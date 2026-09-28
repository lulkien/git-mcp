//! Security primitives: secret redaction and argument validation.

pub mod args;
pub mod redact;

pub use args::{
    assert_safe_arg, assert_safe_command_name, assert_safe_ref, assert_safe_remote_name,
};
pub use redact::{redact_config_value, redact_error, redact_token, redact_url};
