//! A minimal, safe-ish wrapper over the OS WinHTTP stack.
//!
//! This is the "borrow the OS" trick IDM uses: no bundled libcurl / OpenSSL.
//! Networking, TLS (schannel) and connection handling all come from Windows.
//! We declare the handful of WinHTTP entry points we need and link winhttp.lib.

#![allow(non_camel_case_types, non_snake_case, dead_code)]

use std::ffi::c_void;
use std::io;
use std::ptr;

type HINTERNET = *mut c_void;
type BOOL = i32;
type DWORD = u32;
type WORD = u16;
type INTERNET_PORT = WORD;

#[link(name = "winhttp")]
extern "system" {
    fn WinHttpOpen(
        user_agent: *const u16,
        access_type: DWORD,
        proxy: *const u16,
        proxy_bypass: *const u16,
        flags: DWORD,
    ) -> HINTERNET;
    fn WinHttpConnect(
        session: HINTERNET,
        server: *const u16,
        port: INTERNET_PORT,
        reserved: DWORD,
    ) -> HINTERNET;
    fn WinHttpOpenRequest(
        connect: HINTERNET,
        verb: *const u16,
        object_name: *const u16,
        version: *const u16,
        referrer: *const u16,
        accept_types: *const *const u16,
        flags: DWORD,
    ) -> HINTERNET;
    fn WinHttpSendRequest(
        request: HINTERNET,
        headers: *const u16,
        headers_length: DWORD,
        optional: *const c_void,
        optional_length: DWORD,
        total_length: DWORD,
        context: usize,
    ) -> BOOL;
    fn WinHttpReceiveResponse(request: HINTERNET, reserved: *const c_void) -> BOOL;
    fn WinHttpQueryHeaders(
        request: HINTERNET,
        info_level: DWORD,
        name: *const u16,
        buffer: *mut c_void,
        buffer_length: *mut DWORD,
        index: *mut DWORD,
    ) -> BOOL;
    fn WinHttpReadData(
        request: HINTERNET,
        buffer: *mut c_void,
        bytes_to_read: DWORD,
        bytes_read: *mut DWORD,
    ) -> BOOL;
    fn WinHttpSetOption(
        handle: HINTERNET,
        option: DWORD,
        buffer: *const c_void,
        buffer_length: DWORD,
    ) -> BOOL;
    fn WinHttpCloseHandle(handle: HINTERNET) -> BOOL;
}

// ---- constants ----
const WINHTTP_ACCESS_TYPE_NO_PROXY: DWORD = 1;
const WINHTTP_FLAG_SECURE: DWORD = 0x0080_0000;
const WINHTTP_QUERY_RAW_HEADERS_CRLF: DWORD = 22;
const WINHTTP_QUERY_FLAG_NUMBER: DWORD = 0x2000_0000;
const WINHTTP_QUERY_STATUS_CODE: DWORD = 19;
const WINHTTP_OPTION_SECURE_PROTOCOLS: DWORD = 84;
const WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2: DWORD = 0x0000_0800;
const WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3: DWORD = 0x0000_2000;
const ERROR_INSUFFICIENT_BUFFER: i32 = 122;

/// RAII wrapper around an HINTERNET.
pub struct Handle(HINTERNET);

impl Handle {
    fn check(ptr: HINTERNET) -> io::Result<Self> {
        if ptr.is_null() {
            Err(last_error("WinHTTP handle"))
        } else {
            Ok(Self(ptr))
        }
    }
    fn raw(&self) -> HINTERNET {
        self.0
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }
}

fn last_error(what: &str) -> io::Error {
    let os = io::Error::last_os_error();
    io::Error::new(os.kind(), format!("{what}: {os}"))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A WinHTTP session (connection pool + TLS/config context).
pub struct Session {
    handle: Handle,
}

impl Session {
    pub fn open(user_agent: &str) -> io::Result<Self> {
        let ua = wide(user_agent);
        let handle = unsafe { Handle::check(WinHttpOpen(ua.as_ptr(), WINHTTP_ACCESS_TYPE_NO_PROXY, ptr::null(), ptr::null(), 0))? };
        // Prefer modern TLS; fall back silently if the option is unsupported.
        let protos = WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2 | WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3;
        unsafe {
            WinHttpSetOption(
                handle.raw(),
                WINHTTP_OPTION_SECURE_PROTOCOLS,
                &protos as *const DWORD as *const c_void,
                std::mem::size_of::<DWORD>() as DWORD,
            );
        }
        Ok(Self { handle })
    }

    pub fn connect(&self, host: &str, port: u16) -> io::Result<Connection> {
        let w = wide(host);
        let h = unsafe { Handle::check(WinHttpConnect(self.handle.raw(), w.as_ptr(), port, 0))? };
        Ok(Connection { handle: h })
    }
}

pub struct Connection {
    handle: Handle,
}

impl Connection {
    /// Open a request. `verb` e.g. "GET"/"HEAD". `path` is the request target
    /// (e.g. "/file.zip?a=b"). `extra_headers` is a raw "K: V\r\nK2: V2" string.
    pub fn request(&self, verb: &str, path: &str, secure: bool, extra_headers: &str) -> io::Result<Request> {
        let v = wide(verb);
        let p = wide(path);
        let h = wide(extra_headers);
        let flags = if secure { WINHTTP_FLAG_SECURE } else { 0 };
        let handle = unsafe {
            Handle::check(WinHttpOpenRequest(
                self.handle.raw(),
                v.as_ptr(),
                p.as_ptr(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                flags,
            ))?
        };
        Ok(Request { handle, headers_wide: h })
    }
}

pub struct Request {
    handle: Handle,
    headers_wide: Vec<u16>,
}

impl Request {
    pub fn send(&self, total_length: DWORD) -> io::Result<()> {
        let headers_ptr = if self.headers_wide.len() > 1 {
            self.headers_wide.as_ptr()
        } else {
            ptr::null()
        };
        let ok = unsafe {
            WinHttpSendRequest(
                self.handle.raw(),
                headers_ptr,
                (self.headers_wide.len().saturating_sub(1)) as DWORD,
                ptr::null(),
                0,
                total_length,
                0,
            )
        };
        if ok == 0 {
            return Err(last_error("WinHttpSendRequest"));
        }
        let ok = unsafe { WinHttpReceiveResponse(self.handle.raw(), ptr::null()) };
        if ok == 0 {
            return Err(last_error("WinHttpReceiveResponse"));
        }
        Ok(())
    }

    /// HTTP status code.
    pub fn status(&self) -> io::Result<u16> {
        let mut value: DWORD = 0;
        let mut len = std::mem::size_of::<DWORD>() as DWORD;
        let ok = unsafe {
            WinHttpQueryHeaders(
                self.handle.raw(),
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                ptr::null(),
                &mut value as *mut DWORD as *mut c_void,
                &mut len,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(last_error("WinHttpQueryHeaders(status)"));
        }
        Ok(value as u16)
    }

    /// All response headers as a single CRLF-delimited UTF-8 string.
    pub fn headers_text(&self) -> io::Result<String> {
        let mut len: DWORD = 0;
        // First call to get required size (fails with INSUFFICIENT_BUFFER).
        unsafe {
            WinHttpQueryHeaders(
                self.handle.raw(),
                WINHTTP_QUERY_RAW_HEADERS_CRLF,
                ptr::null(),
                ptr::null_mut(),
                &mut len,
                ptr::null_mut(),
            );
        }
        if len == 0 || len > (4 << 20) {
            return Err(io::Error::new(io::ErrorKind::Other, "bad header length"));
        }
        let mut buf: Vec<u16> = vec![0u16; (len as usize / 2) + 2];
        let mut buflen = (buf.len() * 2) as DWORD;
        let ok = unsafe {
            WinHttpQueryHeaders(
                self.handle.raw(),
                WINHTTP_QUERY_RAW_HEADERS_CRLF,
                ptr::null(),
                buf.as_mut_ptr() as *mut c_void,
                &mut buflen,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(last_error("WinHttpQueryHeaders(raw)"));
        }
        let n = buflen as usize / 2;
        Ok(String::from_utf16_lossy(&buf[..n]))
    }

    /// Read up to `buf.len()` bytes into `buf`; returns bytes read (0 = EOF).
    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut read: DWORD = 0;
        let ok = unsafe {
            WinHttpReadData(
                self.handle.raw(),
                buf.as_mut_ptr() as *mut c_void,
                buf.len() as DWORD,
                &mut read,
            )
        };
        if ok == 0 {
            return Err(last_error("WinHttpReadData"));
        }
        Ok(read as usize)
    }
}

/// The ERROR_INSUFFICIENT_BUFFER code, exported for header-size probing tests.
pub const _INSUFFICIENT_BUFFER: i32 = ERROR_INSUFFICIENT_BUFFER;
