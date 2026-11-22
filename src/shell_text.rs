use {
    lazy_regex::regex_is_match,
    serde::Deserialize,
    std::{
        borrow::Borrow,
        fmt,
    },
};

/// The name of a shell alias or function, which can be written
/// as is in shell code
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "String")]
pub struct ShellName(String);

/// The arguments an alias gives to `gg`: words separated by spaces,
/// which can be written as is in shell code
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct AliasTarget(String);

#[derive(Debug, thiserror::Error)]
pub enum ShellTextError {
    #[error(
        "`{0}` isn't a valid alias or function name: it must be made of letters, \
        digits and `_:.+-`, and not start with `-`"
    )]
    Name(String),
    #[error(
        "`{0}` isn't a valid alias target: it must be made of words separated by \
        a space, with only letters, digits and `_:.+@%=,/-`"
    )]
    Target(String),
}

impl TryFrom<String> for ShellName {
    type Error = ShellTextError;
    fn try_from(name: String) -> Result<Self, Self::Error> {
        if regex_is_match!(r"^[\w:.+][\w:.+-]*$", &name) {
            Ok(Self(name))
        } else {
            Err(ShellTextError::Name(name))
        }
    }
}

impl TryFrom<String> for AliasTarget {
    type Error = ShellTextError;
    fn try_from(target: String) -> Result<Self, Self::Error> {
        if regex_is_match!(r"^[\w:.+@%=,/-]+( [\w:.+@%=,/-]+)*$", &target) {
            Ok(Self(target))
        } else {
            Err(ShellTextError::Target(target))
        }
    }
}

impl ShellName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AliasTarget {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn words(&self) -> impl Iterator<Item = &str> {
        self.0.split(' ')
    }
}

impl Borrow<str> for ShellName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ShellName {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for AliasTarget {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[test]
fn test_shell_name() {
    for name in ["gs", ":e", "g.s", "g-s", "g+", "3", "\u{e9}t\u{e9}", "_x"] {
        assert!(ShellName::try_from(name.to_string()).is_ok(), "{name}");
    }
    for name in [
        "", "-x", "g s", "g;s", "g'", "g\"", "$x", "g=h", "g\n", "a/b", "g`",
    ] {
        assert!(ShellName::try_from(name.to_string()).is_err(), "{name}");
    }
}

#[test]
fn test_alias_target() {
    for target in ["status", "diff --cached", "log -n 3 --format=%h", "e a/b.c"] {
        assert!(
            AliasTarget::try_from(target.to_string()).is_ok(),
            "{target}"
        );
    }
    for target in [
        "", " status", "status ", "a  b", "a;b", "a 'b'", "$(x)", "a\nb", "a|b",
    ] {
        assert!(
            AliasTarget::try_from(target.to_string()).is_err(),
            "{target}"
        );
    }
}
