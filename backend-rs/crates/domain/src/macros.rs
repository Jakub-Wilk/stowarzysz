//! Small helpers shared by the bounded contexts.

/// A closed set of names stored as text: `as_str` / `parse` / `ALL`, `snake_case` JSON and (with
/// the `schema` feature) an OpenAPI schema.
macro_rules! text_enum {
    // `Name as SchemaName { ... }`: the OpenAPI schema gets another name (two contexts have a
    // `Status`, and OpenAPI component names must be unique).
    ($(#[$meta:meta])* $name:ident as $schema:ident { $($(#[$vmeta:meta])* $variant:ident = $text:literal),+ $(,)? }) => {
        text_enum!(@define [$(#[$meta])*] [#[cfg_attr(feature = "schema", schema(as = $schema))]] $name { $($(#[$vmeta])* $variant = $text),+ });
    };
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident = $text:literal),+ $(,)? }) => {
        text_enum!(@define [$(#[$meta])*] [] $name { $($(#[$vmeta])* $variant = $text),+ });
    };
    (@define [$(#[$meta:meta])*] [$(#[$schema_meta:meta])*] $name:ident { $($(#[$vmeta:meta])* $variant:ident = $text:literal),+ }) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord,
            ::serde::Serialize, ::serde::Deserialize,
        )]
        #[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
        $(#[$schema_meta])*
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($(#[$vmeta])* $variant),+ }

        impl $name {
            pub const ALL: &[Self] = &[$(Self::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }

            pub fn parse(text: &str) -> Option<Self> {
                match text { $($text => Some(Self::$variant),)+ _ => None }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}
