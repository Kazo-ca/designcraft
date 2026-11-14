# Hosting DesignCraft for the web

`designcraft-web-<version>.zip` (from the GitHub release, or `packaging/web/package.sh`) holds a
static site in `designcraft-web-<version>/`:

| File | What it is |
|---|---|
| `index.html` | The page. It loads everything through relative URLs. |
| `designcraft-web-<hash>.js` | wasm-bindgen glue (generated, ES module) |
| `designcraft-web-<hash>_bg.wasm` | The app, about 13 MB, or 5 MB with compression |
| `_headers`, `.htaccess` | Sample header rules for Netlify/Cloudflare Pages and Apache |

There is no server-side code. Upload the folder's contents anywhere that serves static files.

## Any path works

All URLs in `index.html` are relative (`public_url = "./"` in `apps/designcraft-web/Trunk.toml`),
so the site works at a domain root (`https://example.com/`), under a prefix
(`https://example.com/tools/designcraft/`) and from a CDN bucket. The asset names carry a content
hash, so they can be cached forever. Only `index.html` needs revalidation.

## Required server settings

- **MIME type:** serve `.wasm` as `application/wasm`. Browsers refuse to stream-compile it under
  any other type, and the app then loads slowly or not at all. Serve `.js` as `text/javascript`.
  Most hosts already do both. For nginx, check that `mime.types` has `application/wasm wasm;`.
- **Compression:** turn on gzip or Brotli for `.wasm`, `.js` and `.html`. That takes the
  download from about 13 MB to about 5 MB. You can also precompress (`brotli -k *.wasm`) and let
  the server send `Content-Encoding: br`.
- **Caching:** `Cache-Control: public, max-age=31536000, immutable` on the hashed `.wasm` and
  `.js` files, and `no-cache` on `index.html`.
- **HTTPS:** WebGPU (and the clipboard) only work in a secure context, which means `https://`
  or `http://localhost`. Over plain HTTP elsewhere, the app falls back to WebGL2.
- **No special isolation headers:** DesignCraft doesn't use `SharedArrayBuffer`, so it doesn't
  need `Cross-Origin-Opener-Policy` or `Cross-Origin-Embedder-Policy`. If your site already sends
  COEP `require-corp`, also send `Cross-Origin-Resource-Policy: same-origin` (or `cross-origin`
  when the files live on a CDN) on the app's files.

nginx example:

```nginx
location /designcraft/ {
    types { application/wasm wasm; text/javascript js; text/html html; }
    gzip on;
    gzip_types application/wasm text/javascript text/html;
    location ~* \.(wasm|js)$ { add_header Cache-Control "public, max-age=31536000, immutable"; }
    location ~* index\.html$ { add_header Cache-Control "no-cache"; }
}
```

Local test: `python3 -m http.server 8765` inside the folder, then open http://localhost:8765/.

## Embedding in a page (iframe)

```html
<iframe
  src="https://example.com/designcraft/"
  title="DesignCraft image editor"
  style="width: 100%; height: 720px; border: 0;"
  allow="fullscreen; clipboard-read; clipboard-write"
  allowfullscreen>
</iframe>
```

- The app fills the iframe and follows its size, so size the iframe and not the app.
- Keyboard shortcuts go to the iframe after the user clicks into it, as with any embedded app.
- **Cross-origin embeds** work. Preferences are kept in the iframe's `localStorage`. Browsers
  that partition or block third-party storage may forget them between visits, and the app
  then starts with defaults.
- **Sandboxed iframes** need at least
  `sandbox="allow-scripts allow-same-origin allow-downloads allow-popups"`. Without
  `allow-same-origin` there's no storage. Without `allow-downloads`, Save and Export (browser
  downloads) are blocked.
- Don't send `X-Frame-Options: DENY` or a `frame-ancestors` CSP that excludes the embedding page.

## Host mode: a page that opens and keeps the documents

A page can host DesignCraft and keep its documents itself, instead of the browser's file picker
and downloads: put `?host=parent` on a same-origin iframe's `src`, or `data-host="page"` on the
canvas when the page loads the app into its own document. The messages are the `craft-web-host`
contract (version "1") the other Craft web builds share, with the `designcraft:` prefix
(`apps/designcraft-web/src/host.rs`):

| Message | Direction | Meaning |
|---|---|---|
| `designcraft:ready { version: "1", appVersion, file: false }` | app → page | The app listens: send a file now (as bytes) |
| `designcraft:open { name, bytes }` | page → app | Open the file in a new tab, as File › Open does. A `.designcraft` keeps `name` as its file, so Save writes back to it; an IDML or InDesign file is converted and has none. Files over 1 GiB are refused |
| `designcraft:save { name, bytes, saveAs, id }` | app → page | A save of the document itself (Save, Save As, Save before closing), always a `.designcraft`. `saveAs` is `false` when the document is written over its own file, `true` for Save As (even under the same name) and the first save of a new or converted document |
| `designcraft:saved { id }` | page → app | The file was stored: only then is the document saved (under that name) and does what waited for the save (closing it) go on. Without it the document stays modified |
| `designcraft:dirty { dirty }` | app → page | Unsaved changes appeared in an open document, or are all gone |
| `designcraft:failed { error }` | app → page | The app couldn't start, couldn't open a file, or lost its graphics |

Exports, Save a Copy and batch export still download. In host mode the start screen, the app bar,
About and the Help menu leave out the ArtCraft community links and Check for Updates, and About
says "Based on DesignCraft by the ArtCraft team." Without a host, nothing changes.

## Renderer selection and fallback flags

DesignCraft renders with wgpu. It uses **WebGPU** when the browser has it and falls back to
**WebGL2** on its own. URL query flags override this, and they work on the iframe `src` too:

| Flag | Effect |
|---|---|
| *(none)* | WebGPU if available, otherwise WebGL2 |
| `?webgl` | Force the WebGL2 backend (useful when a WebGPU driver misbehaves) |
| `?cpu` | Force the CPU canvas path (slowest, most compatible) |
| `?control` | Install `window.designcraft`, the page command API (`docs/control-protocol.md`) |

For example: `<iframe src="https://example.com/designcraft/?webgl" ...>`.

A browser with neither WebGPU nor WebGL2 gets a message in place of the app.
