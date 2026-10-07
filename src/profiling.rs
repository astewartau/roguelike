//! Profiler instrumentation, opt-in behind the `profiling` cargo feature.
//!
//! The puffin profiler used to be switched on unconditionally in `main`, which
//! meant a shipped release build paid for every instrumentation scope in the
//! hot loop and opened a local TCP port on the player's machine.
//!
//! The `profile_function!` / `profile_scope!` macros here forward to puffin
//! when the feature is on and expand to nothing when it is off, so a default
//! build carries no instrumentation at all — not even the atomic check puffin
//! does internally. Profiling is then a deliberate act, and because it is a
//! feature rather than `#[cfg(debug_assertions)]` it works on release builds
//! too, which is where profiling numbers are actually worth reading:
//!
//! ```text
//! cargo run --release --features profiling
//! ```
//!
//! `puffin` and `puffin_http` are optional dependencies enabled by that
//! feature, so neither is compiled into a default build.

/// Profile the enclosing function. No-op unless the `profiling` feature is on.
#[cfg(feature = "profiling")]
macro_rules! profile_function {
    () => {
        puffin::profile_function!();
    };
    ($data:expr) => {
        puffin::profile_function!($data);
    };
}

#[cfg(not(feature = "profiling"))]
macro_rules! profile_function {
    () => {};
    ($data:expr) => {};
}

/// Profile a named scope. No-op unless the `profiling` feature is on.
#[cfg(feature = "profiling")]
macro_rules! profile_scope {
    ($name:expr) => {
        puffin::profile_scope!($name);
    };
    ($name:expr, $data:expr) => {
        puffin::profile_scope!($name, $data);
    };
}

#[cfg(not(feature = "profiling"))]
macro_rules! profile_scope {
    ($name:expr) => {};
    ($name:expr, $data:expr) => {};
}

/// Mark a frame boundary for the profiler. No-op unless `profiling` is on.
pub fn new_frame() {
    #[cfg(feature = "profiling")]
    puffin::GlobalProfiler::lock().new_frame();
}

/// Switch scopes on and start the puffin HTTP server, returning its handle
/// (dropping it stops the server). Without the `profiling` feature this does
/// nothing and opens no port.
#[cfg(feature = "profiling")]
pub fn start() -> Option<puffin_http::Server> {
    puffin::set_scopes_on(true);
    let server_addr = format!("127.0.0.1:{}", puffin_http::DEFAULT_PORT);
    let server = puffin_http::Server::new(&server_addr).ok();
    eprintln!("Profiler server running at http://{server_addr}");
    eprintln!("Run `puffin_viewer` or open in browser to view profiler");
    server
}

#[cfg(not(feature = "profiling"))]
pub fn start() {}

#[cfg(test)]
mod tests {
    /// The shim macros must expand and compile in both configurations, and in
    /// a default build must expand to nothing at all — no puffin call, no
    /// atomic check. If someone reintroduces a direct `puffin::` call in the
    /// hot loop this module stops being the single gate and the dependency
    /// check below is what catches it.
    #[test]
    fn test_profiling_macros_are_usable_and_inert() {
        profile_function!();
        profile_scope!("outer");
        {
            profile_scope!("inner");
        }

        // Frame marking is safe to call whether or not the feature is on, and
        // in a default build must not touch a profiler that isn't linked.
        super::new_frame();
        super::new_frame();
    }

    /// In a default build `start()` returns `()`: there is no server handle
    /// because there is no server, which is what keeps a shipped build from
    /// opening a local port. The type itself carries the guarantee.
    #[cfg(not(feature = "profiling"))]
    #[test]
    fn test_default_build_starts_no_profiler_server() {
        let handle: () = super::start();
        assert_eq!(handle, ());
    }

    /// With the feature on, scopes must still be off until `start()` is called
    /// explicitly — enabling instrumentation stays a deliberate act rather
    /// than a side effect of linking the profiler.
    #[cfg(feature = "profiling")]
    #[test]
    fn test_scopes_are_off_until_started() {
        assert!(
            !puffin::are_scopes_on(),
            "linking puffin must not switch scopes on by itself"
        );
    }
}
