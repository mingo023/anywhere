use super::{Reach, Space, offline_note, offline_title};
use crate::desktop::Desktop;
use crate::sidebar::aside::OfflineFrame;

impl Desktop {
    /// Under Projects, in a host's space it can't reach: what its link is doing, and Reconnect.
    pub(crate) fn offline_frame(&self, space: &Space) -> Option<OfflineFrame> {
        let (id, reach) = (space.host()?, self.space_reach(space));
        if reach == Reach::Online {
            return None;
        }
        Some(OfflineFrame {
            glyph: self.space_glyph(space),
            title: offline_title(reach, &self.space_name(space)),
            note: offline_note(self.fleet.hosts.links.get(id), &self.added_name(id)),
            reconnect: (reach == Reach::Offline).then(|| id.to_string()),
        })
    }

    /// A reachable space with no project yet: its name.
    pub(crate) fn empty_space_name(&self, space: &Space) -> Option<String> {
        (self.space_reach(space) == Reach::Online).then(|| self.space_name(space))
    }
}
