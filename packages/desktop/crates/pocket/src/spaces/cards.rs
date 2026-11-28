use super::{Reach, Space};
use crate::desktop::Desktop;

impl Desktop {
    pub(crate) fn empty_space_name(&self, space: &Space) -> Option<String> {
        (self.space_reach(space) == Reach::Online).then(|| self.space_name(space))
    }
}
