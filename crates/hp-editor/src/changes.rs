use crate::Id;
use crate::ids::{IdSet, id_set};

#[derive(Clone, Debug)]
pub struct Changes {
    pub solids_changed: IdSet,
    pub solids_removed: IdSet,
    pub entities_changed: IdSet,
    pub entities_removed: IdSet,
    pub groups_changed: bool,
    pub visgroups_changed: bool,
    pub world_changed: bool,
    pub extra_blocks_changed: bool,
    pub selection_changed: bool,
    pub visibility_changed: bool,
    pub everything: bool,
}

impl Default for Changes {
    fn default() -> Self {
        Self {
            solids_changed: id_set(0),
            solids_removed: id_set(0),
            entities_changed: id_set(0),
            entities_removed: id_set(0),
            groups_changed: false,
            visgroups_changed: false,
            world_changed: false,
            extra_blocks_changed: false,
            selection_changed: false,
            visibility_changed: false,
            everything: false,
        }
    }
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.solids_changed.is_empty()
            && self.solids_removed.is_empty()
            && self.entities_changed.is_empty()
            && self.entities_removed.is_empty()
            && !self.groups_changed
            && !self.visgroups_changed
            && !self.world_changed
            && !self.extra_blocks_changed
            && !self.selection_changed
            && !self.visibility_changed
            && !self.everything
    }

    pub fn solids_dirty(&self) -> bool {
        self.everything || !self.solids_changed.is_empty() || !self.solids_removed.is_empty()
    }

    pub(crate) fn solid_changed(&mut self, id: Id) {
        self.solids_removed.swap_remove(&id);
        self.solids_changed.insert(id);
    }

    pub(crate) fn solid_removed(&mut self, id: Id) {
        self.solids_changed.swap_remove(&id);
        self.solids_removed.insert(id);
    }

    pub(crate) fn entity_changed(&mut self, id: Id) {
        self.entities_removed.swap_remove(&id);
        self.entities_changed.insert(id);
    }

    pub(crate) fn entity_removed(&mut self, id: Id) {
        self.entities_changed.swap_remove(&id);
        self.entities_removed.insert(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_and_removed_are_exclusive() {
        let mut c = Changes::default();
        assert!(c.is_empty());
        c.solid_changed(1);
        c.solid_removed(1);
        assert!(!c.solids_changed.contains(&1) && c.solids_removed.contains(&1));
        c.solid_changed(1);
        assert!(c.solids_changed.contains(&1) && !c.solids_removed.contains(&1));
        c.entity_removed(2);
        c.entity_changed(2);
        assert!(c.entities_changed.contains(&2) && !c.entities_removed.contains(&2));
        assert!(c.solids_dirty());
        assert!(!c.is_empty());
    }
}
