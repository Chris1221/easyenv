use crate::state::{EasyenvState, ManagedVar, PriorValue};
use indexmap::IndexMap;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Export(String, String),
    Unset(String),
}

/// Computes the shell operations needed to move from `prev` (what easyenv
/// previously set) to `target` (the freshly merged `.env` layers for the
/// new cwd), given `current_shell_snapshot` (the values of every key that
/// is either already managed or about to become managed, as observed in the
/// calling shell's actual environment).
///
/// Invariant: a key's `prior` is "the value it had before easyenv ever
/// touched it," captured exactly once and carried forward unchanged across
/// however many intermediate cd's/overrides happen, and only cleared once
/// the key drops out of every `.env` layer. This single rule handles
/// arbitrarily deep parent/child override chains with no special-casing.
pub fn compute_diff(
    prev: &EasyenvState,
    current_shell_snapshot: &HashMap<String, String>,
    target: &IndexMap<String, String>,
) -> (Vec<Op>, BTreeMap<String, ManagedVar>) {
    let mut ops = Vec::new();
    let mut new_managed = BTreeMap::new();

    for (k, v) in target {
        let prior = match prev.managed.get(k) {
            Some(m) => m.prior.clone(),
            None => match current_shell_snapshot.get(k) {
                Some(existing) => PriorValue::Set(existing.clone()),
                None => PriorValue::Unset,
            },
        };
        if current_shell_snapshot.get(k) != Some(v) {
            ops.push(Op::Export(k.clone(), v.clone()));
        }
        new_managed.insert(k.clone(), ManagedVar { prior });
    }

    for (k, m) in &prev.managed {
        if !target.contains_key(k) {
            match &m.prior {
                PriorValue::Set(orig) => ops.push(Op::Export(k.clone(), orig.clone())),
                PriorValue::Unset => ops.push(Op::Unset(k.clone())),
            }
        }
    }

    (ops, new_managed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn target(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn fresh_export_of_previously_unset_key() {
        let prev = EasyenvState::empty();
        let shell = snapshot(&[]);
        let tgt = target(&[("FOO", "1")]);
        let (ops, managed) = compute_diff(&prev, &shell, &tgt);
        assert_eq!(ops, vec![Op::Export("FOO".to_string(), "1".to_string())]);
        assert_eq!(managed["FOO"].prior, PriorValue::Unset);
    }

    #[test]
    fn fresh_export_overriding_users_existing_value() {
        let prev = EasyenvState::empty();
        let shell = snapshot(&[("FOO", "user_value")]);
        let tgt = target(&[("FOO", "env_value")]);
        let (ops, managed) = compute_diff(&prev, &shell, &tgt);
        assert_eq!(
            ops,
            vec![Op::Export("FOO".to_string(), "env_value".to_string())]
        );
        assert_eq!(
            managed["FOO"].prior,
            PriorValue::Set("user_value".to_string())
        );
    }

    #[test]
    fn removal_restores_prior_user_value() {
        let mut managed = BTreeMap::new();
        managed.insert(
            "FOO".to_string(),
            ManagedVar {
                prior: PriorValue::Set("user_value".to_string()),
            },
        );
        let prev = EasyenvState {
            managed,
            signature: 0,
        };
        let shell = snapshot(&[("FOO", "env_value")]);
        let tgt = target(&[]); // cd'd out, no longer in any layer
        let (ops, managed) = compute_diff(&prev, &shell, &tgt);
        assert_eq!(
            ops,
            vec![Op::Export("FOO".to_string(), "user_value".to_string())]
        );
        assert!(managed.is_empty());
    }

    #[test]
    fn removal_unsets_key_that_never_existed_before() {
        let mut managed = BTreeMap::new();
        managed.insert(
            "FOO".to_string(),
            ManagedVar {
                prior: PriorValue::Unset,
            },
        );
        let prev = EasyenvState {
            managed,
            signature: 0,
        };
        let shell = snapshot(&[("FOO", "env_value")]);
        let tgt = target(&[]);
        let (ops, managed) = compute_diff(&prev, &shell, &tgt);
        assert_eq!(ops, vec![Op::Unset("FOO".to_string())]);
        assert!(managed.is_empty());
    }

    #[test]
    fn value_change_while_already_managed_updates_but_keeps_original_prior() {
        let mut managed = BTreeMap::new();
        managed.insert(
            "FOO".to_string(),
            ManagedVar {
                prior: PriorValue::Set("original_user_value".to_string()),
            },
        );
        let prev = EasyenvState {
            managed,
            signature: 0,
        };
        let shell = snapshot(&[("FOO", "old_env_value")]);
        let tgt = target(&[("FOO", "new_env_value")]);
        let (ops, managed) = compute_diff(&prev, &shell, &tgt);
        assert_eq!(
            ops,
            vec![Op::Export("FOO".to_string(), "new_env_value".to_string())]
        );
        assert_eq!(
            managed["FOO"].prior,
            PriorValue::Set("original_user_value".to_string())
        );
    }

    #[test]
    fn no_op_when_value_unchanged() {
        let mut managed = BTreeMap::new();
        managed.insert(
            "FOO".to_string(),
            ManagedVar {
                prior: PriorValue::Unset,
            },
        );
        let prev = EasyenvState {
            managed,
            signature: 0,
        };
        let shell = snapshot(&[("FOO", "same")]);
        let tgt = target(&[("FOO", "same")]);
        let (ops, managed) = compute_diff(&prev, &shell, &tgt);
        assert!(ops.is_empty());
        assert_eq!(managed["FOO"].prior, PriorValue::Unset);
    }

    /// The key scenario from the plan: parent sets FOO=1, child overrides to
    /// FOO=2. Entering the child while already in the parent (parent's
    /// export already materialized) must not re-capture "1" as the prior to
    /// restore -- prior must stay whatever was true before easyenv touched
    /// FOO at all.
    #[test]
    fn child_overrides_parent_then_cd_out_restores_pre_easyenv_value() {
        // Step 1: enter parent dir, FOO=1, user had no prior FOO set.
        let prev0 = EasyenvState::empty();
        let shell0 = snapshot(&[]);
        let tgt0 = target(&[("FOO", "1")]);
        let (ops0, managed0) = compute_diff(&prev0, &shell0, &tgt0);
        assert_eq!(ops0, vec![Op::Export("FOO".to_string(), "1".to_string())]);
        let state0 = EasyenvState {
            managed: managed0,
            signature: 0,
        };

        // Step 2: cd into child, FOO overridden to 2. Shell now reflects
        // FOO=1 (from step 1's export).
        let shell1 = snapshot(&[("FOO", "1")]);
        let tgt1 = target(&[("FOO", "2")]);
        let (ops1, managed1) = compute_diff(&state0, &shell1, &tgt1);
        assert_eq!(ops1, vec![Op::Export("FOO".to_string(), "2".to_string())]);
        // Prior must still be Unset (the value before easyenv ever touched
        // FOO), NOT Set("1").
        assert_eq!(managed1["FOO"].prior, PriorValue::Unset);
        let state1 = EasyenvState {
            managed: managed1,
            signature: 0,
        };

        // Step 3: cd back out to parent. FOO should restore to "1", not
        // unset entirely, since the parent layer still supplies FOO.
        let shell2 = snapshot(&[("FOO", "2")]);
        let tgt2 = target(&[("FOO", "1")]);
        let (ops2, managed2) = compute_diff(&state1, &shell2, &tgt2);
        assert_eq!(ops2, vec![Op::Export("FOO".to_string(), "1".to_string())]);
        assert_eq!(managed2["FOO"].prior, PriorValue::Unset);
        let state2 = EasyenvState {
            managed: managed2,
            signature: 0,
        };

        // Step 4: cd all the way out. FOO should unset entirely (it was
        // never set by the user before easyenv).
        let shell3 = snapshot(&[("FOO", "1")]);
        let tgt3 = target(&[]);
        let (ops3, managed3) = compute_diff(&state2, &shell3, &tgt3);
        assert_eq!(ops3, vec![Op::Unset("FOO".to_string())]);
        assert!(managed3.is_empty());
    }

    #[test]
    fn reentry_is_idempotent() {
        let mut managed = BTreeMap::new();
        managed.insert(
            "FOO".to_string(),
            ManagedVar {
                prior: PriorValue::Unset,
            },
        );
        let prev = EasyenvState {
            managed,
            signature: 0,
        };
        let shell = snapshot(&[("FOO", "1")]);
        let tgt = target(&[("FOO", "1")]);
        let (ops, managed) = compute_diff(&prev, &shell, &tgt);
        assert!(ops.is_empty());
        assert_eq!(managed.len(), 1);
    }
}
