use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use std::collections::HashSet;

/// Provides some conveniences for querying a `kube-rs` object.
pub trait CrdExt {
    /// Returns this objects `ObjectMeta` information (i.e. the `metadata` field). You implement
    /// this be returning `&self.metadata`. This allows the rest of this trait's functions to be
    /// implemented for you.
    fn object_meta(&self) -> &ObjectMeta;

    /// Returns the object.metadata.name field, unwrapping a potential `None` with `""`. In
    /// practice, an object's name cannot be missing since this is how we `GET` an object in the
    /// first place, so we do away with the `Option` for convenience. This is named `object_name`
    /// to avoid confusion with `ResourceExt`.
    fn object_name(&self) -> &str {
        self.object_meta().name.as_deref().unwrap_or("")
    }

    /// Returns this object's YAML representation as a String.
    fn to_yaml(&self) -> Result<String, serde_yaml::Error>;

    /// Duplicate finalizers are problematic so we want to interact with them as a unique set.
    fn finalizer_set(&self) -> HashSet<String> {
        let option_vec = self.object_meta().finalizers.as_ref();
        option_vec
            .map(|vec| {
                vec.iter()
                    .map(|s| s.to_owned())
                    .collect::<HashSet<String>>()
            })
            .unwrap_or_else(HashSet::new)
    }

    /// Does the object have one or more finalizers.
    fn has_finalizers(&self) -> bool {
        self.object_meta()
            .finalizers
            .as_ref()
            .map(|finalizers| !finalizers.is_empty())
            .unwrap_or(false)
    }

    /// Does the object have the given `finalizer`.
    fn has_finalizer(&self, finalizer: &str) -> bool {
        let mut finalizers = match &self.object_meta().finalizers {
            None => return false,
            Some(value) => value.iter(),
        };
        finalizers.any(|item| item == finalizer)
    }

    /// Does the object have the given `finalizer`, and in what position.
    fn finalizer_position(&self, finalizer: &str) -> Option<usize> {
        let mut finalizers = match &self.object_meta().finalizers {
            None => return None,
            Some(value) => value.iter(),
        };
        finalizers.position(|item| item == finalizer)
    }

    /// Has someone requested that the object be deleted.
    fn is_delete_requested(&self) -> bool {
        self.object_meta().deletion_timestamp.is_some()
    }
}
