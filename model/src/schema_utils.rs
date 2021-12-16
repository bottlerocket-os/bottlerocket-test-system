use schemars::{gen::SchemaGenerator, schema::Schema, JsonSchema};
use serde::{Deserialize, Deserializer};

/// Instead of making new struct model fields `Option`s, we can use this function when deserializing
/// to assign the default value. This makes the structs more ergonomic to use, and makes yaml/json
/// representations backward compatible.
pub(crate) fn null_to_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    let opt = Option::deserialize(d)?;
    let val = opt.unwrap_or_default();
    Ok(val)
}

/// In OpenAPI, a [nullable enum] must have the enum value "null" in it's list of allowed strings.
/// This function takes the `Schema` from an enum type, marks it as `nullable` and adds the string
/// `null` to the list of allowed strings.
///
/// [nullable enum]: https://swagger.io/docs/specification/data-models/enums
pub(crate) fn nullable_enum<T>(g: &mut SchemaGenerator) -> Schema
where
    T: JsonSchema,
{
    // let mut schema_name = DestructionPolicy::schema_name();
    let mut schema = match T::json_schema(g) {
        // This shouldn't happen
        Schema::Bool(x) => return Schema::Bool(x),
        Schema::Object(schema_object) => schema_object,
    };
    if let Some(enum_values) = &mut schema.enum_values {
        enum_values.push(serde_json::Value::String("null".to_owned()))
    }
    schema
        .extensions
        .insert("nullable".to_owned(), serde_json::Value::Bool(true));

    schema.into()
}
