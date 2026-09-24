use anyhow::{Context, Result, bail};
use regex::Regex;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::LazyLock,
};

#[derive(Clone, Debug)]
pub struct Device {
    pub udid: String,
    pub name: String,
    pub product: String,
    pub version: String,
}

fn command(name: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(name).args(args).output().with_context(|| {
        format!("`{name}` was not found. Install libimobiledevice and ensure it is in PATH.")
    })?;
    if !output.status.success() {
        bail!(
            "`{name}` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn list_devices() -> Result<Vec<Device>> {
    let ids = command("idevice_id", &["-l"]).map_err(|error| {
        if !Path::new("/run/usbmuxd").exists() {
            anyhow::anyhow!(
                "iPhone USB service (usbmuxd) is unavailable. Install/start usbmuxd on the host, reconnect the iPhone, then retry: {error}"
            )
        } else {
            error
        }
    })?;
    ids.lines()
        .filter(|id| !id.trim().is_empty())
        .map(|id| {
            let udid = id.trim().to_string();
            let name = command("ideviceinfo", &["-u", &udid, "-k", "DeviceName"])
                .unwrap_or_else(|_| "Unknown".into());
            let product = command("ideviceinfo", &["-u", &udid, "-k", "ProductType"])
                .unwrap_or_else(|_| "Unknown".into());
            let version = command("ideviceinfo", &["-u", &udid, "-k", "ProductVersion"])
                .unwrap_or_else(|_| "Unknown".into());
            Ok(Device {
                udid,
                name,
                product,
                version,
            })
        })
        .collect()
}

pub fn tools_status() -> String {
    match command("idevice_id", &["--version"]) {
        Ok(_) => "libimobiledevice is available".into(),
        Err(e) => e.to_string(),
    }
}

pub fn prepare_skin(source: &Path) -> Result<PathBuf> {
    let img =
        image::open(source).with_context(|| format!("Could not open {}", source.display()))?;
    let (w, h) = image::GenericImageView::dimensions(&img);
    if w == 0 || h == 0 {
        bail!("Image has invalid dimensions");
    }
    let target_ratio = 1536.0 / 969.0;
    let source_ratio = w as f64 / h as f64;
    let crop = if source_ratio > target_ratio {
        let cw = (h as f64 * target_ratio).round() as u32;
        img.crop_imm((w - cw) / 2, 0, cw, h)
    } else {
        let ch = (w as f64 / target_ratio).round() as u32;
        img.crop_imm(0, (h - ch) / 2, w, ch)
    };
    let output =
        std::env::temp_dir().join(format!("aircard-skin-{:032x}.png", rand::random::<u128>()));
    crop.resize_exact(1536, 969, image::imageops::FilterType::Lanczos3)
        .save(&output)?;
    Ok(output)
}

pub fn theme_summary(path: &Path) -> Result<usize> {
    let file = fs::File::open(path).context("Could not open theme")?;
    let mut zip = zip::ZipArchive::new(file).context("A .passthm file must be a ZIP archive")?;
    let mut count = 0;
    for n in 0..zip.len() {
        let entry = zip.by_index(n)?;
        let name = entry.name().to_ascii_lowercase();
        if name.ends_with(".png") || name.ends_with(".jpg") || name.ends_with(".jpeg") {
            count += 1;
        }
    }
    if count == 0 {
        bail!("No keypad image assets found in theme");
    }
    Ok(count)
}

pub fn extract_card_hashes(log: &str) -> Vec<String> {
    static PATH: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:Cards|Passes/Cards)/([A-Za-z0-9+/_-]{27,43}={0,2})(?:\.pkpass|/|\s|$)")
            .unwrap()
    });
    static DASHBOARD: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"Passbook\(PassKitUI\).*Dashboard loading \([^)]*\): ([A-Za-z0-9+/_-]{27,43}={0,2})(?:\s|$)").unwrap()
    });
    PATH.captures_iter(log)
        .chain(DASHBOARD.captures_iter(log))
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_owned()))
        .collect()
}

pub fn stage_apply(
    kind: &str,
    udid: &str,
    payload: &Path,
    card_hash: Option<&str>,
) -> Result<String> {
    match kind {
        "skin" => {
            crate::linux_backend::apply_skin(udid, payload, card_hash.context("Missing card hash")?)
        }
        "theme" => crate::linux_backend::apply_theme(udid, payload),
        _ => bail!("Unknown apply operation"),
    }
}

#[cfg(test)]
mod tests {
    use super::extract_card_hashes;

    #[test]
    fn detects_wallet_card_hash_from_log_path() {
        let hash = "M6nDwZrkYbFlsodLgCbvyFZQ1cc=";
        let log = format!("passd loaded /var/mobile/Library/Passes/Cards/{hash}.pkpass");
        assert_eq!(extract_card_hashes(&log), vec![hash]);
    }

    #[test]
    fn detects_wallet_card_hash_from_dashboard_loading() {
        let hash = "BHHUbzHV5Lf5GxSe4ceD43tiwl8=";
        let log = format!(
            "Sep 25 06:59:03 Passbook(PassKitUI)[25069] <Notice>: Dashboard loading (0x7545961900): {hash} - m:NO, sm:YES"
        );
        assert_eq!(extract_card_hashes(&log), vec![hash]);
    }
}
