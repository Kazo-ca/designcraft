use super::{ControlRequest, DesignApp, Receiver};

impl DesignApp {
    pub fn with_control(mut self, rx: Receiver<ControlRequest>) -> Self {
        self.control_rx = Some(rx);
        self
    }

    /// Defer control commands while host-order input is pending; otherwise handle replies and screenshot admission.
    pub(crate) fn drain_control(&mut self, ctx: &egui::Context) {
        // An injected batch may still be waiting in the host's pending raw input. Running another
        // control command now could switch documents or dialogs under it, so keep the channel
        // queued until a settled UI pass has taken the batch.
        if self.host_order_pending() {
            return;
        }
        let Some(rx) = self.control_rx.take() else { return };
        while !self.host_order_pending() {
            let Ok(req) = rx.try_recv() else { break };
            let reply = req.reply.clone();
            // The transport stopped waiting for it (a page's call timed out): don't run it now.
            if !req.dispatch.start() {
                let _ = reply.send(serde_json::json!({"ok": false, "error": crate::control::NOT_RUN}));
                continue;
            }
            match crate::control::handle(self, ctx, &req) {
                crate::control::Outcome::Done(v) => {
                    let _ = reply.send(v);
                }
                crate::control::Outcome::Screenshot { path, data } => {
                    if let Err(e) = self.admit_screenshot_with(path, data, reply.clone()) {
                        let _ = reply.send(e);
                    }
                }
            }
        }
        self.control_rx = Some(rx);
    }
}
