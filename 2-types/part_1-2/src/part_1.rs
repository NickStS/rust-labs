#[derive(Debug, Clone)]
pub struct PostCommon {
    pub id: u64,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct New(pub PostCommon);

#[derive(Debug, Clone)]
pub struct Unmoderated(pub PostCommon);

#[derive(Debug, Clone)]
pub struct Published(pub PostCommon);

#[derive(Debug, Clone)]
pub struct Deleted(pub PostCommon);

impl New {
    pub fn new(id: u64, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self(PostCommon {
            id,
            title: title.into(),
            body: body.into(),
        })
    }

    pub fn publish(self) -> Unmoderated {
        Unmoderated(self.0)
    }
}

impl Unmoderated {
    pub fn allow(self) -> Published {
        Published(self.0)
    }

    pub fn deny(self) -> Deleted {
        Deleted(self.0)
    }

    pub fn delete(self) -> Deleted {
        Deleted(self.0)
    }
}

impl Published {
    pub fn delete(self) -> Deleted {
        Deleted(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_allow_publish() {
        let new = New::new(1, "Hello", "Body");
        let unmod = new.publish();
        let published = unmod.allow();
        assert_eq!(published.0.id, 1);
        let deleted = published.delete();
        assert_eq!(deleted.0.title, "Hello");
    }

    #[test]
    fn flow_deny() {
        let new = New::new(2, "T", "B");
        let unmod = new.publish();
        let deleted = unmod.deny();
        assert_eq!(deleted.0.id, 2);
    }
}
