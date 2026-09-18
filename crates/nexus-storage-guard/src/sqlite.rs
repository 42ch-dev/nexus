//! SQLite FFI: install connection-local writer protocol scalar functions.

use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::panic::catch_unwind;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use libsqlite3_sys::{
    sqlite3, sqlite3_context, sqlite3_create_function_v2, sqlite3_result_int64,
    sqlite3_result_null, sqlite3_result_text, sqlite3_user_data, sqlite3_value, SQLITE_INNOCUOUS,
    SQLITE_OK, SQLITE_TRANSIENT, SQLITE_UTF8,
};
use sqlx::sqlite::SqliteConnection;

use crate::{WriterConnectionContext, WriterMode};

/// Live FFI userdata boxes in this process — one per registration that supplied
/// a destructor and has not been destroyed yet.
///
/// This is the ownership invariant itself: every `sqlite3_create_function_v2`
/// attempt adds exactly one box and `destroy_userdata` removes exactly one, so
/// the count returns to its previous value once a connection is torn down. It
/// is compiled only into debug builds and exists so the ownership proof can be
/// *asserted* from a fixture instead of inferred from observable values.
#[cfg(debug_assertions)]
static LIVE_REGISTRATIONS: AtomicUsize = AtomicUsize::new(0);

impl WriterConnectionContext {
    /// Live FFI userdata boxes in this process (debug builds only).
    #[cfg(debug_assertions)]
    #[must_use]
    pub fn live_registration_count() -> usize {
        LIVE_REGISTRATIONS.load(Ordering::SeqCst)
    }
}

/// Per-registration userdata: one `Arc` clone per successful `sqlite3_create_function_v2`.
struct UserData(Arc<WriterConnectionContext>);

/// Drop one registration's userdata box.
///
/// SQLite invokes this exactly once per registration that supplied it — including
/// when `sqlite3_create_function_v2` **fails** (bundled `sqlite3.h:5444-5452`). The
/// error path must therefore never call this manually.
unsafe extern "C" fn destroy_userdata(ptr: *mut c_void) {
    if !ptr.is_null() {
        #[cfg(debug_assertions)]
        LIVE_REGISTRATIONS.fetch_sub(1, Ordering::SeqCst);
        // SAFETY: pointer came from `Box::into_raw` in `register_scalar`.
        drop(unsafe { Box::from_raw(ptr.cast::<UserData>()) });
    }
}

fn context_from(ctx: *mut sqlite3_context) -> Option<Arc<WriterConnectionContext>> {
    let ptr = unsafe { sqlite3_user_data(ctx) };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: userdata is a valid `UserData` box until `destroy_userdata` runs.
    Some(Arc::clone(&unsafe { &*ptr.cast::<UserData>() }.0))
}

extern "C" fn scalar_writer_id(
    ctx: *mut sqlite3_context,
    _argc: c_int,
    _argv: *mut *mut sqlite3_value,
) {
    let _ = catch_unwind(|| {
        if let Some(data) = context_from(ctx) {
            unsafe {
                sqlite3_result_text(
                    ctx,
                    data.writer_id.as_ptr().cast::<c_char>(),
                    data.writer_id.len() as c_int,
                    SQLITE_TRANSIENT(),
                );
            }
        } else {
            unsafe { sqlite3_result_null(ctx) };
        }
    });
}

extern "C" fn scalar_writer_protocol(
    ctx: *mut sqlite3_context,
    _argc: c_int,
    _argv: *mut *mut sqlite3_value,
) {
    let _ = catch_unwind(|| {
        if let Some(data) = context_from(ctx) {
            unsafe { sqlite3_result_int64(ctx, data.protocol_version as i64) };
        } else {
            unsafe { sqlite3_result_null(ctx) };
        }
    });
}

extern "C" fn scalar_writer_mode(
    ctx: *mut sqlite3_context,
    _argc: c_int,
    _argv: *mut *mut sqlite3_value,
) {
    let _ = catch_unwind(|| {
        if let Some(data) = context_from(ctx) {
            let mode = match data.mode {
                WriterMode::Direct => "direct",
                WriterMode::Engine => "engine",
                WriterMode::Migration => "migration",
            };
            unsafe {
                sqlite3_result_text(
                    ctx,
                    mode.as_ptr().cast::<c_char>(),
                    mode.len() as c_int,
                    SQLITE_TRANSIENT(),
                );
            }
        } else {
            unsafe { sqlite3_result_null(ctx) };
        }
    });
}

extern "C" fn scalar_migration_epoch(
    ctx: *mut sqlite3_context,
    _argc: c_int,
    _argv: *mut *mut sqlite3_value,
) {
    let _ = catch_unwind(|| {
        if let Some(data) = context_from(ctx) {
            unsafe { sqlite3_result_int64(ctx, data.migration_epoch) };
        } else {
            unsafe { sqlite3_result_null(ctx) };
        }
    });
}

extern "C" fn scalar_engine_epoch(
    ctx: *mut sqlite3_context,
    _argc: c_int,
    _argv: *mut *mut sqlite3_value,
) {
    let _ = catch_unwind(|| {
        if let Some(data) = context_from(ctx) {
            match data.engine_epoch {
                Some(epoch) => unsafe { sqlite3_result_int64(ctx, epoch) },
                None => unsafe { sqlite3_result_null(ctx) },
            }
        } else {
            unsafe { sqlite3_result_null(ctx) };
        }
    });
}

type ScalarFn = unsafe extern "C" fn(*mut sqlite3_context, c_int, *mut *mut sqlite3_value);

fn register_scalar(
    db: *mut sqlite3,
    name: &str,
    func: ScalarFn,
    context: Arc<WriterConnectionContext>,
) -> Result<(), sqlx::Error> {
    let c_name =
        CString::new(name).map_err(|e| sqlx::Error::Configuration(e.to_string().into()))?;
    let userdata = Box::into_raw(Box::new(UserData(context)));
    #[cfg(debug_assertions)]
    LIVE_REGISTRATIONS.fetch_add(1, Ordering::SeqCst);
    let status = unsafe {
        sqlite3_create_function_v2(
            db,
            c_name.as_ptr(),
            0,
            SQLITE_UTF8 | SQLITE_INNOCUOUS,
            userdata.cast::<c_void>(),
            Some(func),
            None,
            None,
            Some(destroy_userdata),
        )
    };
    if status == SQLITE_OK {
        Ok(())
    } else {
        // Registration failed: SQLite already invoked `destroy_userdata` once.
        // Manual free here would double-free (sqlite3.h:5444-5452).
        Err(sqlx::Error::Configuration(
            format!("sqlite3_create_function_v2({name}) failed: {status}").into(),
        ))
    }
}

/// Install all five writer protocol scalar functions on `conn`.
///
/// # Ownership proof
///
/// One shared [`Arc<WriterConnectionContext>`] is cloned into a distinct
/// `Box<UserData>` per registration. Each box carries its own `destroy_userdata`
/// callback. On connection close (or per-function delete) SQLite drops every
/// successful registration's box, decrementing the `Arc` until the payload is
/// freed exactly once.
///
/// On partial failure of registration *N*, SQLite invokes `destroy_userdata` on
/// that attempt's box only; earlier successful registrations retain live `Arc`
/// clones and their userdata pointers stay valid until connection teardown.
/// The error path never calls `destroy_userdata` manually.
///
/// Debug builds expose the same count through
/// [`WriterConnectionContext::live_registration_count`], so a fixture asserts
/// the arithmetic (one box per attempt, one destructor per box) instead of
/// inferring it from returned values.
pub async fn install_writer_functions(
    conn: &mut SqliteConnection,
    context: WriterConnectionContext,
) -> Result<(), sqlx::Error> {
    let shared = Arc::new(context);
    let mut handle = conn.lock_handle().await?;
    let db = handle.as_raw_handle().as_ptr();
    let functions: [(&str, ScalarFn); 5] = [
        ("nexus_writer_id", scalar_writer_id),
        ("nexus_writer_protocol", scalar_writer_protocol),
        ("nexus_writer_mode", scalar_writer_mode),
        ("nexus_migration_epoch", scalar_migration_epoch),
        ("nexus_engine_epoch", scalar_engine_epoch),
    ];
    for (name, func) in functions {
        register_scalar(db, name, func, Arc::clone(&shared))?;
    }
    Ok(())
}
