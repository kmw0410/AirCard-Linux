//! Linux AirTraffic transport. Protocol layout follows the MIT-licensed
//! AirCard-iOS Rust core and AirCard-Windows staging implementation.
use anyhow::{Context, Result, bail};
use plist::{Dictionary, Value};
use std::os::unix::fs::PermissionsExt;
use std::{
    ffi::{CString, c_char, c_void},
    io::{Cursor, Read, Write},
    path::Path,
    ptr, thread,
    time::Duration,
};
use zip::{
    CompressionMethod, ZipWriter,
    write::{ExtendedFileOptions, FileOptions},
};

type Handle = *mut c_void;

#[link(name = "imobiledevice-1.0")]
unsafe extern "C" {
    fn idevice_new(device: *mut Handle, udid: *const c_char) -> i32;
    fn idevice_free(device: Handle) -> i32;
    fn lockdownd_client_new_with_handshake(
        device: Handle,
        client: *mut Handle,
        label: *const c_char,
    ) -> i32;
    fn lockdownd_client_free(client: Handle) -> i32;
    fn lockdownd_start_service(
        client: Handle,
        service: *const c_char,
        descriptor: *mut Handle,
    ) -> i32;
    fn lockdownd_service_descriptor_free(descriptor: Handle) -> i32;
    fn afc_client_new(device: Handle, descriptor: Handle, client: *mut Handle) -> i32;
    fn afc_client_free(client: Handle) -> i32;
    fn afc_get_file_info(client: Handle, path: *const c_char, info: *mut *mut *mut c_char) -> i32;
    fn afc_dictionary_free(info: *mut *mut c_char) -> i32;
    fn afc_file_open(client: Handle, path: *const c_char, mode: u32, file: *mut u64) -> i32;
    fn afc_file_close(client: Handle, file: u64) -> i32;
    fn afc_file_read(
        client: Handle,
        file: u64,
        data: *mut c_char,
        len: u32,
        received: *mut u32,
    ) -> i32;
    fn afc_file_write(
        client: Handle,
        file: u64,
        data: *const c_char,
        len: u32,
        written: *mut u32,
    ) -> i32;
    fn afc_make_directory(client: Handle, path: *const c_char) -> i32;
    fn afc_read_directory(
        client: Handle,
        path: *const c_char,
        entries: *mut *mut *mut c_char,
    ) -> i32;
    fn afc_remove_path(client: Handle, path: *const c_char) -> i32;
    fn service_client_new(device: Handle, descriptor: Handle, client: *mut Handle) -> i32;
    fn service_client_free(client: Handle) -> i32;
    fn service_send(client: Handle, data: *const c_char, len: u32, sent: *mut u32) -> i32;
    fn service_receive_with_timeout(
        client: Handle,
        data: *mut c_char,
        len: u32,
        received: *mut u32,
        timeout: u32,
    ) -> i32;
}

fn ok(code: i32, action: &str) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        bail!("{action} failed (libimobiledevice code {code})")
    }
}
fn cstr(value: &str) -> Result<CString> {
    Ok(CString::new(value).context("Path contains a NUL byte")?)
}

struct Device {
    udid: String,
    raw: Handle,
    lockdown: Handle,
    afc: Handle,
}
impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            if !self.afc.is_null() {
                afc_client_free(self.afc);
            }
            if !self.lockdown.is_null() {
                lockdownd_client_free(self.lockdown);
            }
            if !self.raw.is_null() {
                idevice_free(self.raw);
            }
        }
    }
}
impl Device {
    fn open(udid: &str) -> Result<Self> {
        let mut this = Self {
            udid: udid.to_string(),
            raw: ptr::null_mut(),
            lockdown: ptr::null_mut(),
            afc: ptr::null_mut(),
        };
        ok(
            unsafe { idevice_new(&mut this.raw, cstr(udid)?.as_ptr()) },
            "Open selected iPhone",
        )?;
        ok(
            unsafe {
                lockdownd_client_new_with_handshake(
                    this.raw,
                    &mut this.lockdown,
                    cstr("AirCard Linux")?.as_ptr(),
                )
            },
            "Pairing handshake",
        )?;
        let desc = this.start_descriptor("com.apple.afc")?;
        let status = unsafe { afc_client_new(this.raw, desc, &mut this.afc) };
        unsafe {
            lockdownd_service_descriptor_free(desc);
        }
        ok(status, "Open AFC")?;
        Ok(this)
    }
    fn start_descriptor(&self, name: &str) -> Result<Handle> {
        let mut desc = ptr::null_mut();
        ok(
            unsafe { lockdownd_start_service(self.lockdown, cstr(name)?.as_ptr(), &mut desc) },
            &format!("Start {name}"),
        )?;
        if desc.is_null() {
            bail!("{name} returned no service descriptor");
        }
        Ok(desc)
    }
    fn service(&self, name: &str) -> Result<Service> {
        let desc = self.start_descriptor(name)?;
        let mut raw = ptr::null_mut();
        let status = unsafe { service_client_new(self.raw, desc, &mut raw) };
        unsafe {
            lockdownd_service_descriptor_free(desc);
        }
        ok(status, &format!("Connect {name}"))?;
        Ok(Service { raw })
    }
    fn file_info(&self, path: &str) -> Result<Vec<(String, String)>> {
        let mut info: *mut *mut c_char = ptr::null_mut();
        ok(
            unsafe { afc_get_file_info(self.afc, cstr(path)?.as_ptr(), &mut info) },
            &format!("AFC stat {path}"),
        )?;
        let mut fields = Vec::new();
        if !info.is_null() {
            unsafe {
                let mut pos = 0;
                while !(*info.add(pos)).is_null() && !(*info.add(pos + 1)).is_null() {
                    let key = std::ffi::CStr::from_ptr(*info.add(pos))
                        .to_string_lossy()
                        .into_owned();
                    let value = std::ffi::CStr::from_ptr(*info.add(pos + 1))
                        .to_string_lossy()
                        .into_owned();
                    fields.push((key, value));
                    pos += 2;
                }
                afc_dictionary_free(info);
            }
        }
        Ok(fields)
    }
    fn exists(&self, path: &str) -> bool {
        self.file_info(path).is_ok()
    }
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let mut raw_info: *mut *mut c_char = ptr::null_mut();
        let status = unsafe { afc_get_file_info(self.afc, cstr(path)?.as_ptr(), &mut raw_info) };
        if !raw_info.is_null() {
            unsafe {
                afc_dictionary_free(raw_info);
            }
        }
        if status == 8 {
            return Ok(None);
        }
        ok(status, &format!("AFC stat {path}"))?;
        let info = match self.file_info(path) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        let size: usize = info
            .iter()
            .find(|(k, _)| k == "st_size")
            .and_then(|(_, v)| v.parse().ok())
            .context("AFC file size unavailable")?;
        if size > 32 * 1024 * 1024 {
            bail!("Books snapshot file is too large: {path}");
        }
        let mut file = 0;
        ok(
            unsafe { afc_file_open(self.afc, cstr(path)?.as_ptr(), 1, &mut file) },
            &format!("AFC open {path}"),
        )?;
        let result = (|| {
            let mut data = Vec::with_capacity(size);
            while data.len() < size {
                let mut chunk = vec![0u8; (size - data.len()).min(65536)];
                let mut got = 0;
                ok(
                    unsafe {
                        afc_file_read(
                            self.afc,
                            file,
                            chunk.as_mut_ptr().cast(),
                            chunk.len() as u32,
                            &mut got,
                        )
                    },
                    "AFC read",
                )?;
                if got == 0 {
                    bail!("AFC read ended early for {path}");
                }
                data.extend_from_slice(&chunk[..got as usize]);
            }
            Ok(data)
        })();
        unsafe {
            afc_file_close(self.afc, file);
        }
        result.map(Some)
    }
    fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        let mut file = 0;
        ok(
            unsafe { afc_file_open(self.afc, cstr(path)?.as_ptr(), 3, &mut file) },
            &format!("AFC open {path}"),
        )?;
        let result = (|| {
            for chunk in data.chunks(65536) {
                let mut offset = 0;
                while offset < chunk.len() {
                    let mut written = 0;
                    ok(
                        unsafe {
                            afc_file_write(
                                self.afc,
                                file,
                                chunk[offset..].as_ptr().cast(),
                                (chunk.len() - offset) as u32,
                                &mut written,
                            )
                        },
                        "AFC write",
                    )?;
                    if written == 0 {
                        bail!("AFC write made no progress");
                    }
                    offset += written as usize;
                }
            }
            Ok(())
        })();
        unsafe {
            afc_file_close(self.afc, file);
        }
        result
    }
    fn mkdirs(&self, path: &str) -> Result<()> {
        let mut current = String::new();
        for component in path.split('/') {
            if component.is_empty() {
                continue;
            }
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(component);
            if !self.exists(&current) {
                ok(
                    unsafe { afc_make_directory(self.afc, cstr(&current)?.as_ptr()) },
                    &format!("AFC mkdir {current}"),
                )?;
            }
        }
        Ok(())
    }
    fn remove(&self, path: &str) -> Result<()> {
        self.remove_inner(path, 0)
    }
    fn remove_inner(&self, path: &str, depth: usize) -> Result<()> {
        if depth > 32 {
            bail!("AFC directory nesting is too deep");
        }
        let info = match self.file_info(path) {
            Ok(info) => info,
            Err(_) => return Ok(()),
        };
        let kind = info
            .iter()
            .find(|(key, _)| key == "st_ifmt")
            .map(|(_, value)| value.as_str());
        if kind == Some("S_IFDIR") {
            let mut entries: *mut *mut c_char = ptr::null_mut();
            ok(
                unsafe { afc_read_directory(self.afc, cstr(path)?.as_ptr(), &mut entries) },
                &format!("AFC list {path}"),
            )?;
            if !entries.is_null() {
                unsafe {
                    let mut pos = 0;
                    while !(*entries.add(pos)).is_null() {
                        let name = std::ffi::CStr::from_ptr(*entries.add(pos))
                            .to_string_lossy()
                            .into_owned();
                        if name != "." && name != ".." {
                            self.remove_inner(&format!("{path}/{name}"), depth + 1)?;
                        }
                        pos += 1;
                    }
                    afc_dictionary_free(entries);
                }
            }
        }
        self.unlink(path)
    }
    fn unlink(&self, path: &str) -> Result<()> {
        if !self.exists(path) {
            return Ok(());
        }
        ok(
            unsafe { afc_remove_path(self.afc, cstr(path)?.as_ptr()) },
            &format!("AFC unlink {path}"),
        )
    }
}

/// Verify the native device connection without modifying iPhone files.
pub fn probe_device(udid: &str) -> Result<()> {
    let _device = Device::open(udid)?;
    Ok(())
}

struct Service {
    raw: Handle,
}
impl Drop for Service {
    fn drop(&mut self) {
        unsafe {
            if !self.raw.is_null() {
                service_client_free(self.raw);
            }
        }
    }
}
impl Service {
    fn send(&mut self, data: &[u8]) -> Result<()> {
        for chunk in data.chunks(65536) {
            let mut offset = 0;
            while offset < chunk.len() {
                let mut sent = 0;
                ok(
                    unsafe {
                        service_send(
                            self.raw,
                            chunk[offset..].as_ptr().cast(),
                            (chunk.len() - offset) as u32,
                            &mut sent,
                        )
                    },
                    "Service send",
                )?;
                if sent == 0 {
                    bail!("Service send made no progress");
                }
                offset += sent as usize;
            }
        }
        Ok(())
    }
    fn recv_exact(&mut self, len: usize, timeout_ms: u32) -> Result<Vec<u8>> {
        let mut data = vec![0u8; len];
        let mut cursor = 0;
        while cursor < len {
            let mut received = 0;
            ok(
                unsafe {
                    service_receive_with_timeout(
                        self.raw,
                        data[cursor..].as_mut_ptr().cast(),
                        (len - cursor) as u32,
                        &mut received,
                        timeout_ms,
                    )
                },
                "Service receive",
            )?;
            if received == 0 {
                bail!("Service closed unexpectedly");
            }
            cursor += received as usize;
        }
        Ok(data)
    }
    fn recv_plist(&mut self, little_endian: bool, timeout_ms: u32) -> Result<Value> {
        let header = self.recv_exact(4, timeout_ms)?;
        let len = if little_endian {
            u32::from_le_bytes(header.try_into().unwrap())
        } else {
            u32::from_be_bytes(header.try_into().unwrap())
        } as usize;
        if len == 0 || len > 10 * 1024 * 1024 {
            bail!("Invalid service plist size: {len}");
        }
        Ok(plist::from_bytes(&self.recv_exact(len, timeout_ms)?)?)
    }
    fn send_plist(&mut self, value: &Value, little_endian: bool) -> Result<()> {
        let mut body = Vec::new();
        plist::to_writer_binary(&mut body, value)?;
        let size = u32::try_from(body.len())?;
        self.send(&if little_endian {
            size.to_le_bytes()
        } else {
            size.to_be_bytes()
        })?;
        self.send(&body)
    }
}

const BOOK_FILES: &[&str] = &[
    "Books/Books.plist",
    "Books/Sync/Books.plist",
    "Books/Sync/Upload.plist",
    "Books/Sync/Database/OutstandingAssets_4.sqlite",
    "Books/Sync/Database/OutstandingAssets_4.sqlite-shm",
    "Books/Sync/Database/OutstandingAssets_4.sqlite-wal",
];
fn snapshot(device: &Device) -> Result<Vec<(&'static str, Option<Vec<u8>>)>> {
    let files: Vec<_> = BOOK_FILES
        .iter()
        .map(|p| Ok((*p, device.read(p)?)))
        .collect::<Result<_>>()?;
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|p| std::path::PathBuf::from(p).join(".local/share"))
        })
        .context("Could not find a user data directory for Books backup")?;
    let dir = base
        .join("aircard/backups")
        .join(&device.udid)
        .join(format!(
            "{}-{:08x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs(),
            rand::random::<u32>()
        ));
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Create Books backup at {}", dir.display()))?;
    for private in [
        base.join("aircard"),
        base.join("aircard/backups"),
        base.join("aircard/backups").join(&device.udid),
        dir.clone(),
    ] {
        std::fs::set_permissions(private, std::fs::Permissions::from_mode(0o700))?;
    }
    let mut index = String::new();
    for (path, data) in &files {
        if let Some(data) = data {
            let output = dir.join(path);
            std::fs::create_dir_all(output.parent().unwrap())?;
            std::fs::write(&output, data)?;
            std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o600))?;
            index.push_str(&format!("present {path}\n"));
        } else {
            index.push_str(&format!("absent {path}\n"));
        }
    }
    std::fs::write(dir.join("manifest.txt"), index)?;
    std::fs::set_permissions(
        dir.join("manifest.txt"),
        std::fs::Permissions::from_mode(0o600),
    )?;
    Ok(files)
}
pub fn restore_backup(udid: &str, backup: &Path) -> Result<()> {
    let index = std::fs::read_to_string(backup.join("manifest.txt"))?;
    let mut files = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in index.lines() {
        let (kind, path) = line.split_once(' ').context("Invalid backup manifest")?;
        if !BOOK_FILES.contains(&path) {
            bail!("Unexpected backup path: {path}");
        }
        if !seen.insert(path) {
            bail!("Duplicate backup path: {path}");
        }
        let data = match kind {
            "present" => Some(std::fs::read(backup.join(path))?),
            "absent" => None,
            _ => bail!("Unexpected backup state: {kind}"),
        };
        files.push((path, data));
    }
    if files.len() != BOOK_FILES.len() {
        bail!("Incomplete Books backup");
    }
    let device = Device::open(udid)?;
    restore(&device, files)
}
fn restore(device: &Device, files: Vec<(&str, Option<Vec<u8>>)>) -> Result<()> {
    let mut errors = Vec::new();
    for (path, data) in files {
        let result = if let Some(data) = data {
            let parent = path.rsplit_once('/').unwrap().0;
            device
                .mkdirs(parent)
                .and_then(|_| device.write(path, &data))
        } else {
            device.remove(path)
        };
        if let Err(e) = result {
            errors.push(format!("{path}: {e}"));
        }
    }
    if !errors.is_empty() {
        bail!("Could not restore Books state: {}", errors.join("; "));
    }
    Ok(())
}

fn zip_opts(mode: u32) -> FileOptions<'static, ExtendedFileOptions> {
    let mut opts = FileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(mode);
    let _ = opts.add_extra_data(0x5A53, Box::new((mode as u16).to_le_bytes()), false);
    opts
}
fn archive(target: &str, files: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let tail = target
        .strip_prefix('/')
        .context("Target must be absolute")?;
    let mut out = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut out);
    zip.add_directory("META-INF/", zip_opts(0o040755))?;
    zip.start_file("META-INF/com.apple.ZipMetadata.plist", zip_opts(0o100600))?;
    let mut meta = Dictionary::new();
    meta.insert("Version".into(), Value::Integer(2.into()));
    let mut metadata = Vec::new();
    plist::to_writer_binary(&mut metadata, &Value::Dictionary(meta))?;
    zip.write_all(&metadata)?;
    for name in ["p0/", "p0/p1/", "p0/p1/p2/"] {
        zip.add_directory(name, zip_opts(0o040755))?;
    }
    zip.add_symlink(
        "p0/p1/p2/link",
        format!("../../../{tail}"),
        zip_opts(0o120777),
    )?;
    let mut path = String::new();
    for segment in tail.split('/') {
        if segment.is_empty() {
            continue;
        }
        path.push_str(segment);
        path.push('/');
        zip.add_directory(path.as_str(), zip_opts(0o040755))?;
    }
    for (idx, (_, payload)) in files.iter().enumerate() {
        zip.start_file(format!("payload_{idx}"), zip_opts(0o100600))?;
        zip.write_all(payload)?;
    }
    zip.finish()?;
    Ok(out.into_inner())
}
fn books_manifest(ids: &[String]) -> Result<Vec<u8>> {
    let mut rows = Vec::new();
    for (idx, id) in ids.iter().enumerate() {
        let mut row = Dictionary::new();
        row.insert("Persistent ID".into(), Value::String(id.clone()));
        row.insert("Item ID".into(), Value::String((idx + 1).to_string()));
        row.insert("DSID".into(), Value::String("1".into()));
        rows.push(Value::Dictionary(row));
    }
    let mut root = Dictionary::new();
    root.insert("Books".into(), Value::Array(rows));
    let mut bytes = Vec::new();
    plist::to_writer_binary(&mut bytes, &Value::Dictionary(root))?;
    Ok(bytes)
}

fn atc_msg(name: &str, session: i64, params: Option<Dictionary>) -> Value {
    let mut msg = Dictionary::new();
    msg.insert("Command".into(), Value::String(name.into()));
    msg.insert("Session".into(), Value::Integer(session.into()));
    if let Some(params) = params {
        msg.insert("Params".into(), Value::Dictionary(params));
    }
    Value::Dictionary(msg)
}
fn atc_name(value: &Value) -> Option<&str> {
    value
        .as_dictionary()?
        .get("Command")
        .or_else(|| value.as_dictionary()?.get("MessageName"))?
        .as_string()
}
fn wait_for(service: &mut Service, expected: &[&str], attempts: usize) -> Result<Value> {
    for _ in 0..attempts {
        let value = service.recv_plist(true, 5000)?;
        match atc_name(&value) {
            Some("Ping") => service.send_plist(&atc_msg("Pong", 1, None), true)?,
            Some(name) if expected.contains(&name) => return Ok(value),
            Some("SyncFailed") => continue, // Prior session cancellation may arrive first.
            Some("SyncFinished") => bail!("AirTraffic ended before {}", expected.join("/")),
            _ => {}
        }
    }
    bail!(
        "AirTraffic did not send {}. Unlock the iPhone and open Apple Books once.",
        expected.join("/")
    )
}
fn sync_assets(device: &Device, assets: &[(String, String)]) -> Result<()> {
    let mut atc = device.service("com.apple.atc")?;
    wait_for(&mut atc, &["SyncAllowed"], 15)?;
    // Public test vector from AirCard-iOS GrappaHelper.m (MIT). It is a client
    // request generated by Apple's host framework for protocol version 1.
    const GRAPPA_REQUEST: &str = "01012ba6a01f2ccf66a02613d5b72e0bc916004058a001a6874d18b5bd7b3395e25d79fa3ffcc67e718106d485c51540b828d1620e9f94f582d3bcc6f97e9088c923095ad8d36ab568fb45df61e286d25354b04c";
    let grappa = GRAPPA_REQUEST
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair)?, 16).map_err(Into::into))
        .collect::<Result<Vec<u8>>>()?;
    let mut host = Dictionary::new();
    host.insert("Type".into(), Value::String("iTunes".into()));
    host.insert("Version".into(), Value::String("13.7.0.161".into()));
    host.insert("MacOSVersion".into(), Value::String("15.0".into()));
    host.insert("SyncHostName".into(), Value::String("aircard-linux".into()));
    host.insert(
        "LibraryID".into(),
        Value::String(format!("{:032x}", rand::random::<u128>())),
    );
    host.insert(
        "SyncedDataclasses".into(),
        Value::Array(vec![Value::String("Book".into())]),
    );
    host.insert(
        "SyncedAssetTypes".into(),
        Value::Array(vec![Value::String("Book".into())]),
    );
    host.insert("Wakeable".into(), Value::Boolean(false));
    host.insert("Grappa".into(), Value::Data(grappa.clone()));
    let mut host_params = Dictionary::new();
    host_params.insert("HostInfo".into(), Value::Dictionary(host.clone()));
    host_params.insert("LocalCloudSupport".into(), Value::Boolean(false));
    atc.send_plist(&atc_msg("HostInfo", 0, Some(host_params)), true)?;
    thread::sleep(Duration::from_millis(200));
    let mut request = Dictionary::new();
    request.insert(
        "Dataclasses".into(),
        Value::Array(vec![Value::String("Book".into())]),
    );
    request.insert(
        "DataclassAnchors".into(),
        Value::Dictionary(Dictionary::new()),
    );
    request.insert("HostInfo".into(), Value::Dictionary(host));
    request.insert("Grappa".into(), Value::Data(grappa));
    atc.send_plist(&atc_msg("RequestingSync", 1, Some(request)), true)?;
    let ready = wait_for(&mut atc, &["ReadyForSync", "AssetManifest"], 24)?;
    let mut sync_types = Dictionary::new();
    sync_types.insert("Book".into(), Value::Integer(1.into()));
    let mut metadata = Dictionary::new();
    metadata.insert("SyncTypes".into(), Value::Dictionary(sync_types));
    metadata.insert(
        "DataclassAnchors".into(),
        Value::Dictionary(Dictionary::new()),
    );
    atc.send_plist(&atc_msg("FinishedSyncingMetadata", 1, Some(metadata)), true)?;
    let manifest = if atc_name(&ready) == Some("AssetManifest") {
        ready
    } else {
        wait_for(&mut atc, &["AssetManifest"], 20)?
    };
    let books = manifest
        .as_dictionary()
        .and_then(|d| d.get("Params"))
        .and_then(Value::as_dictionary)
        .and_then(|d| d.get("AssetManifest"))
        .and_then(Value::as_dictionary)
        .and_then(|d| d.get("Book"))
        .and_then(Value::as_array)
        .context("AirTraffic manifest has no Book assets")?;
    for (id, _) in assets {
        let found = books.iter().any(|row| {
            row.as_dictionary().is_some_and(|d| {
                d.get("AssetID").and_then(Value::as_string) == Some(id)
                    && d.get("IsDownload").and_then(Value::as_boolean) == Some(true)
            })
        });
        if !found {
            bail!("AirTraffic did not request staged asset {id}");
        }
    }
    for (idx, (id, destination)) in assets.iter().enumerate() {
        let mut params = Dictionary::new();
        params.insert("AssetID".into(), Value::String(id.clone()));
        params.insert("Dataclass".into(), Value::String("Book".into()));
        params.insert("AssetPath".into(), Value::String(destination.clone()));
        atc.send_plist(&atc_msg("FileComplete", 1, Some(params)), true)?;
        if idx + 1 < assets.len() {
            thread::sleep(Duration::from_millis(900));
        }
    }
    thread::sleep(Duration::from_secs(2));
    Ok(())
}

fn write_batch(device: &Device, target: &str, files: &[(String, Vec<u8>)]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    if !target.starts_with("/var/mobile/") {
        bail!("Unsupported target directory");
    }
    for (name, _) in files {
        if name.is_empty() || name.contains('/') || name == "." || name == ".." {
            bail!("Invalid asset name: {name}");
        }
    }
    let token = format!("{:020x}", rand::random::<u128>() & ((1u128 << 80) - 1));
    let source = format!("airlift-src-{token}");
    let link = format!("airlift-link-{token}");
    let mut ids = vec![format!("../../{source}/p0/p1/p2/link")];
    let mut assets = vec![(ids[0].clone(), link.clone())];
    for (idx, (leaf, _)) in files.iter().enumerate() {
        let id = format!("../../{source}/payload_{idx}");
        ids.push(id.clone());
        assets.push((id, format!("{link}/{leaf}")));
    }
    let archive = archive(target, files)?;
    let books = books_manifest(&ids)?;
    let snapshot = snapshot(device).context("Books state snapshot failed")?;
    let result = (|| -> Result<()> {
        let mut zip = device.service("com.apple.streaming_zip_conduit")?;
        let mut init = Dictionary::new();
        init.insert("MediaSubdir".into(), Value::String(source.clone()));
        zip.send_plist(&Value::Dictionary(init), false)?;
        zip.send(&archive)?;
        let response = zip
            .recv_plist(false, 25000)
            .context("StreamingZip did not confirm extraction")?;
        if let Some(error) = response.as_dictionary().and_then(|d| d.get("Error")) {
            bail!("StreamingZip rejected archive: {error:?}");
        }
        drop(zip);
        if !device.exists(&format!("{source}/p0/p1/p2/link"))
            || !device.exists(&format!("{source}/payload_0"))
        {
            bail!("StreamingZip stage verification failed");
        }
        device.mkdirs("Books/Sync")?;
        device.write("Books/Sync/Books.plist", &books)?;
        sync_assets(device, &assets)?;
        Ok(())
    })();
    let _ = device.unlink(&link);
    let cleanup = device.remove(&source);
    let restored = restore(device, snapshot);
    if let Err(error) = restored {
        return Err(error).context(format!(
            "Books state restore failed after operation: {result:?}"
        ));
    }
    result?;
    cleanup.context("Staged archive cleanup failed")?;
    Ok(())
}

fn invalidate_cache(device: &Device, target: &str) -> Result<usize> {
    let token = format!("{:020x}", rand::random::<u128>() & ((1u128 << 80) - 1));
    let source = format!("airlift-src-{token}");
    let link = format!("airlift-link-{token}");
    let leaves = ["FrontFace", "PlaceHolder", "Preview"];
    let mut ids = vec![format!("../../{source}/p0/p1/p2/link")];
    let mut assets = vec![(ids[0].clone(), link.clone())];
    for (idx, leaf) in leaves.iter().enumerate() {
        let id = format!("../../{link}/{leaf}");
        ids.push(id.clone());
        assets.push((id, format!("{source}/removed-{idx}")));
    }
    let archive = archive(
        target,
        &[("payload".into(), b"aircard-cache-cleanup".to_vec())],
    )?;
    let books = books_manifest(&ids)?;
    let snapshot = snapshot(device)?;
    let result = (|| -> Result<usize> {
        let mut zip = device.service("com.apple.streaming_zip_conduit")?;
        let mut init = Dictionary::new();
        init.insert("MediaSubdir".into(), Value::String(source.clone()));
        zip.send_plist(&Value::Dictionary(init), false)?;
        zip.send(&archive)?;
        zip.recv_plist(false, 25000)?;
        drop(zip);
        if !device.exists(&format!("{source}/p0/p1/p2/link")) {
            bail!("Cache removal stage failed");
        }
        device.mkdirs("Books/Sync")?;
        device.write("Books/Sync/Books.plist", &books)?;
        sync_assets(device, &assets)?;
        Ok((0..leaves.len())
            .filter(|i| device.exists(&format!("{source}/removed-{i}")))
            .count())
    })();
    let _ = device.unlink(&link);
    let _ = device.remove(&source);
    let restored = restore(device, snapshot);
    restored.context("Books state restore after cache invalidation failed")?;
    result
}

fn png_to_pdf(png: &[u8]) -> Result<Vec<u8>> {
    let image = image::load_from_memory(png)?.to_rgb8();
    let (width, height) = image.dimensions();
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(image.as_raw(), 6);
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    let mut object = |body: Vec<u8>| {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        pdf.extend_from_slice(&body);
        pdf.extend_from_slice(b"\nendobj\n");
    };
    object(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    object(b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec());
    object(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Contents 4 0 R /Resources << /XObject << /Im0 5 0 R >> >> >>").into_bytes());
    let content = format!("q\n{width} 0 0 {height} 0 0 cm\n/Im0 Do\nQ\n");
    object(
        format!(
            "<< /Length {} >>\nstream\n{}endstream",
            content.len(),
            content
        )
        .into_bytes(),
    );
    let mut body = format!("<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n", compressed.len()).into_bytes();
    body.extend_from_slice(&compressed);
    body.extend_from_slice(b"\nendstream");
    object(body);
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes(),
    );
    for offset in &offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len() + 1
        )
        .as_bytes(),
    );
    Ok(pdf)
}

pub fn apply_skin(udid: &str, path: &Path, hash: &str) -> Result<String> {
    if !hash
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "+/=_-".contains(c))
        || !(27..=44).contains(&hash.len())
    {
        bail!("Invalid Wallet card hash");
    }
    let png = std::fs::read(path)?;
    let pdf = png_to_pdf(&png)?;
    let device = Device::open(udid)?;
    let target = format!("/var/mobile/Library/Passes/Cards/{hash}.pkpass");
    let files = vec![
        ("cardBackgroundCombined@3x.png".into(), png.clone()),
        ("cardBackgroundCombined@2x.png".into(), png),
        ("cardBackgroundCombined.pdf".into(), pdf),
    ];
    write_batch(&device, &target, &files).context("Card artwork write failed")?;
    let mut cleared = 0;
    for ext in ["cache", "pkcache"] {
        let cache_dir = format!("/var/mobile/Library/Passes/Cards/{hash}.{ext}");
        cleared += invalidate_cache(&device, &cache_dir)
            .with_context(|| format!("Artwork written, but {ext} cache invalidation failed"))?;
    }
    Ok(format!(
        "Artwork sent; {cleared} cache entries removed. Force-close and reopen Wallet to inspect the result."
    ))
}

pub fn apply_theme(udid: &str, path: &Path) -> Result<String> {
    let input = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(input)?;
    let mut images = std::collections::BTreeMap::new();
    for idx in 0..zip.len() {
        let entry = zip.by_index(idx)?;
        let name = entry.name().rsplit('/').next().unwrap_or("").to_string();
        let low = name.to_ascii_lowercase();
        if entry.is_dir()
            || !(low.ends_with(".png") || low.ends_with(".jpg") || low.ends_with(".jpeg"))
        {
            continue;
        }
        if name.is_empty() || name.starts_with('.') {
            continue;
        }
        let mut data = Vec::new();
        entry.take(8 * 1024 * 1024).read_to_end(&mut data)?;
        let digit = name
            .chars()
            .find(|c| c.is_ascii_digit() || *c == '*' || *c == '#');
        if let Some(digit) = digit {
            let png = image::load_from_memory(&data)?.to_rgba8();
            let mut bytes = Vec::new();
            image::DynamicImage::ImageRgba8(png)
                .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)?;
            images.entry(digit).or_insert(bytes);
        }
    }
    if images.is_empty() {
        bail!("Theme contains no supported keypad images");
    }
    let subtexts = [
        "+", "", "A B C", "D E F", "G H I", "J K L", "M N O", "P Q R S", "T U V", "W X Y Z",
    ];
    let mut files = Vec::new();
    for (digit, data) in images {
        let subtext = digit
            .to_digit(10)
            .map(|n| subtexts[n as usize])
            .unwrap_or("");
        for prefix in ["en", "other"] {
            for bold in ["", "-bold"] {
                files.push((format!("{prefix}-{digit}---white{bold}.png"), data.clone()));
                if !subtext.is_empty() {
                    files.push((
                        format!("{prefix}-{digit}-{subtext}--white{bold}.png"),
                        data.clone(),
                    ));
                    if subtext.contains(' ') {
                        files.push((
                            format!(
                                "{prefix}-{digit}-{}--white{bold}.png",
                                subtext.replace(' ', "")
                            ),
                            data.clone(),
                        ));
                    }
                }
            }
        }
    }
    files.push(("_big".into(), Vec::new()));
    let device = Device::open(udid)?;
    for batch in files.chunks(3) {
        write_batch(&device, "/var/mobile/Library/Caches/TelephonyUI-10", batch)
            .context("Passcode theme write failed")?;
    }
    Ok("Theme sent. Restart the iPhone to inspect the result.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_zip_contains_relocation_symlink_and_assets() {
        let files = vec![("cardBackgroundCombined@3x.png".into(), vec![1, 2, 3])];
        let data = archive("/var/mobile/Library/Passes/Cards/test.pkpass", &files).unwrap();
        let mut zip = zip::ZipArchive::new(Cursor::new(data)).unwrap();
        let mut link = String::new();
        zip.by_name("p0/p1/p2/link")
            .unwrap()
            .read_to_string(&mut link)
            .unwrap();
        assert_eq!(link, "../../../var/mobile/Library/Passes/Cards/test.pkpass");
        assert_eq!(
            zip.by_name("p0/p1/p2/link").unwrap().unix_mode().unwrap() & 0o170000,
            0o120000
        );
        let mut payload = Vec::new();
        zip.by_name("payload_0")
            .unwrap()
            .read_to_end(&mut payload)
            .unwrap();
        assert_eq!(payload, vec![1, 2, 3]);
    }

    #[test]
    fn books_manifest_preserves_asset_order() {
        let bytes = books_manifest(&["first".into(), "second".into()]).unwrap();
        let value: Value = plist::from_bytes(&bytes).unwrap();
        let rows = value.as_dictionary().unwrap()["Books"].as_array().unwrap();
        assert_eq!(
            rows[0].as_dictionary().unwrap()["Persistent ID"].as_string(),
            Some("first")
        );
        assert_eq!(
            rows[1].as_dictionary().unwrap()["Item ID"].as_string(),
            Some("2")
        );
    }
}
