use serde::{Deserialize, Serialize};

/// Update instruction for a nullable field: leave it, set it, or clear it.
/// (`Option<T>` alone can't tell "not provided" from "set to null".)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Patch<T> {
    Keep,
    Set(T),
    Clear,
}

// Manual impl: `#[derive(Default)]` would require `T: Default`.
#[allow(clippy::derivable_impls)]
impl<T> Default for Patch<T> {
    fn default() -> Self {
        Patch::Keep
    }
}

impl<T> Patch<T> {
    pub fn apply(self, target: &mut Option<T>) {
        match self {
            Patch::Keep => {}
            Patch::Set(v) => *target = Some(v),
            Patch::Clear => *target = None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply() {
        let mut v = Some(1);
        Patch::Keep.apply(&mut v);
        assert_eq!(v, Some(1));
        Patch::Set(2).apply(&mut v);
        assert_eq!(v, Some(2));
        Patch::Clear.apply(&mut v);
        assert_eq!(v, None);
    }
}
