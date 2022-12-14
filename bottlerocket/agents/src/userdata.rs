// Heavily borrowed from Bottlerocket's merge-toml crate.
// See https://github.com/bottlerocket-os/bottlerocket/blob/v1.11.1/sources/api/storewolf/merge-toml/src/lib.rs

use base64::decode;
use resource_agent::provider::{IntoProviderError, ProviderError, ProviderResult, Resources};
use std::str::from_utf8;
use toml::{map::Entry, Value};

/// This modifies the first given toml Value by inserting any values from the second Value.
///
/// This is done recursively.  Any time a scalar or array is seen, the left side is set to match
/// the right side.  Any time a table is seen, we iterate through the keys of the tables; if the
/// left side does not have the key from the right side, it's inserted, otherwise we recursively
/// merge the values in each table for that key.
///
/// If at any point in the recursion the data types of the two values does not match, we error.
pub fn merge_values<'a>(merge_from: &'a Value, merge_into: &'a mut Value) -> ProviderResult<()> {
    // If the types of left and right don't match, we have inconsistent models, and shouldn't try
    // to merge them.
    if !merge_into.same_type(merge_from) {
        IntoProviderError::context(
            None,
            Resources::Clear,
            "Cannot merge mismatched data types in given TOML",
        )?
    }

    match merge_from {
        // If we see a scalar, we replace the left with the right.  We treat arrays like scalars so
        // behavior is clear - no question about whether we're appending right onto left, etc.
        Value::String(_)
        | Value::Integer(_)
        | Value::Float(_)
        | Value::Boolean(_)
        | Value::Datetime(_)
        | Value::Array(_) => *merge_into = merge_from.clone(),

        // If we see a table, we recursively merge each key.
        Value::Table(from) => {
            // We know the other side is a table because of the `ensure` above.
            let to = merge_into.as_table_mut().ok_or_else(|| {
                ProviderError::new_with_context(
                    Resources::Clear,
                    "Cannot merge mismatched data types in given TOML",
                )
            })?;
            for (k_from, v_from) in from.iter() {
                // Check if the left has the same key as the right.
                match to.entry(k_from) {
                    // If not, we can just insert the value.
                    Entry::Vacant(e) => {
                        e.insert(v_from.clone());
                    }
                    // If so, we need to recursively merge; we don't want to replace an entire
                    // table, for example, because the left may have some distinct inner keys.
                    Entry::Occupied(ref mut e) => {
                        merge_values(v_from, e.get_mut())?;
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn decode_to_string(encoded_userdata: &String) -> ProviderResult<String> {
    Ok(from_utf8(
        &decode(encoded_userdata).context(Resources::Clear, "Failed to decode base64 TOML")?,
    )
    .context(Resources::Clear, "Failed to decode base64 TOML")?
    .to_string())
}

#[cfg(test)]
mod test {
    use super::merge_values;
    use toml::toml;

    #[test]
    fn merge_1() {
        let mut left = toml! {
            top1 = "left top1"
            top2 = "left top2"
            [settings.inner]
            inner_setting1 = "left inner_setting1"
            inner_setting2 = "left inner_setting2"
        };
        let right = toml! {
            top1 = "right top1"
            [settings]
            setting = "right setting"
            [settings.inner]
            inner_setting1 = "right inner_setting1"
            inner_setting3 = "right inner_setting3"
        };
        // Can't comment inside this toml, unfortunately.
        // "top1" is being overwritten from right.
        // "top2" is only in the left and remains.
        // "setting" is only in the right side.
        // "inner" tests that recursion works; inner_setting1 is replaced, 2 is untouched, and
        // 3 is new.
        let expected = toml! {
            top1 = "right top1"
            top2 = "left top2"
            [settings]
            setting = "right setting"
            [settings.inner]
            inner_setting1 = "right inner_setting1"
            inner_setting2 = "left inner_setting2"
            inner_setting3 = "right inner_setting3"
        };
        merge_values(&right, &mut left).unwrap();
        assert_eq!(left, expected);
    }

    #[test]
    fn merge_2() {
        let mut left = toml! {
            top1 = "left top1"
            top2 = "left top2"
            [settings]
            setting = "left setting"
            [settings.inner]
            inner_setting1 = "left inner_setting1"
            inner_setting2 = "left inner_setting2"
        };
        let right = toml! {
            top1 = "right top1"
            [settings.inner]
            inner_setting1 = "right inner_setting1"
            inner_setting3 = "right inner_setting3"
        };
        let expected = toml! {
            top1 = "right top1"
            top2 = "left top2"
            [settings]
            setting = "left setting"
            [settings.inner]
            inner_setting1 = "right inner_setting1"
            inner_setting2 = "left inner_setting2"
            inner_setting3 = "right inner_setting3"
        };
        merge_values(&right, &mut left).unwrap();
        assert_eq!(left, expected);
    }

    #[test]
    fn merge_3() {
        let mut left = toml! {
            top1 = "left top1"
        };
        let right = toml! {
            top1 = 12
        };
        assert!(merge_values(&right, &mut left).is_err());
    }
}
