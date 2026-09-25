use super::*;

pub(super) struct Resolution {
    id: String,
    app: Option<PinnedApp>,
}

pub(super) enum AppVisual {
    Loading,
    Missing,
    Ready {
        app: PinnedApp,
        texture: Option<egui::TextureHandle>,
    },
}

impl AppVisual {
    pub(super) fn texture(&self) -> Option<&egui::TextureHandle> {
        match self {
            Self::Ready { texture, .. } => texture.as_ref(),
            _ => None,
        }
    }
    pub(super) fn dominant(&self) -> Option<[u8; 3]> {
        match self {
            Self::Ready { app, .. } => app.icon.as_ref().map(|icon| icon.dominant),
            _ => None,
        }
    }
    pub(super) fn name(&self) -> Option<&str> {
        match self {
            Self::Ready { app, .. } => Some(&app.name),
            _ => None,
        }
    }
}

impl AppState {
    pub(super) fn queue_application(&mut self, id: &str) {
        if id == "*"
            || id.is_empty()
            || id.len() > crate::config::MAX_APPLICATION_BYTES
            || self.application_assets.contains_key(id)
        {
            return;
        }
        if self.application_assets.len() >= 256 {
            let unused = self
                .application_assets
                .iter()
                .find(|(key, value)| {
                    !matches!(value, AppVisual::Loading)
                        && !self
                            .config
                            .rules
                            .iter()
                            .any(|rule| rule.application == **key)
                        && !self.pinned.iter().any(|app| app.id == **key)
                        && !self.recent_applications.contains(key)
                })
                .map(|(key, _)| key.clone());
            if let Some(key) = unused {
                self.application_assets.remove(&key);
            } else {
                return;
            }
        }
        self.application_assets
            .insert(id.to_owned(), AppVisual::Loading);
        self.application_requests.push(id.to_owned());
    }

    pub(super) fn cache_application(
        &mut self,
        ctx: &egui::Context,
        id: String,
        app: Option<PinnedApp>,
    ) {
        let visual = if let Some(app) = app {
            let texture = app.icon.as_ref().map(|icon| {
                ctx.load_texture(
                    format!("desktop-icon:{id}"),
                    egui::ColorImage::from_rgba_unmultiplied([icon.width, icon.height], &icon.rgba),
                    egui::TextureOptions::LINEAR,
                )
            });
            AppVisual::Ready { app, texture }
        } else {
            AppVisual::Missing
        };
        self.application_assets.insert(id, visual);
    }

    pub(super) fn poll_applications(&mut self, ctx: &egui::Context) {
        if let Some(receiver) = &self.application_scan {
            match receiver.try_recv() {
                Ok(apps) => {
                    self.application_scan = None;
                    for Resolution { id, app } in apps {
                        self.cache_application(ctx, id, app);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.application_scan = None;
                    for visual in self.application_assets.values_mut() {
                        if matches!(visual, AppVisual::Loading) {
                            *visual = AppVisual::Missing;
                        }
                    }
                    self.notify("Shortcut icon lookup stopped. Missing icons remain text-only.");
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.pending_rule.as_ref().is_some_and(|(id, _)| {
            self.application_assets
                .get(id)
                .is_some_and(|asset| !matches!(asset, AppVisual::Loading))
        }) {
            let (id, screen) = self
                .pending_rule
                .take()
                .expect("pending rule checked above");
            self.insert_rule(id, screen);
        }
        let missing: Vec<_> = self
            .config
            .rules
            .iter()
            .map(|rule| &rule.application)
            .chain(self.recent_applications.iter())
            .filter(|id| id.as_str() != "*" && !self.application_assets.contains_key(*id))
            .cloned()
            .collect();
        for id in missing {
            self.queue_application(&id);
        }
        if self.application_scan.is_none() && !self.application_requests.is_empty() {
            let requests = std::mem::take(&mut self.application_requests);
            let (send, receive) = mpsc::sync_channel(1);
            self.application_scan = Some(receive);
            let _ = std::thread::Builder::new()
                .name("ledalert-app-icons".into())
                .spawn(move || {
                    let apps = requests
                        .into_iter()
                        .map(|id| {
                            let app = taskbar::application(&id).ok().flatten();
                            Resolution { id, app }
                        })
                        .collect();
                    let _ = send.send(apps);
                });
        }
    }
}
