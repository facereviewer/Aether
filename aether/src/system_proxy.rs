use std::net::SocketAddr;

#[cfg(target_os = "linux")]
pub fn set_proxy(addr: SocketAddr) -> Result<(), String> {
    let ip = addr.ip().to_string();
    let port = addr.port().to_string();
    let errors: Vec<String> = Vec::new();

    // Try GNOME (gsettings)
    if std::process::Command::new("gsettings").arg("--version").output().is_ok() {
        let _ = run_cmd("gsettings", &["set", "org.gnome.system.proxy", "mode", "manual"]);
        let _ = run_cmd("gsettings", &["set", "org.gnome.system.proxy.http", "host", &ip]);
        let _ = run_cmd("gsettings", &["set", "org.gnome.system.proxy.http", "port", &port]);
        let _ = run_cmd("gsettings", &["set", "org.gnome.system.proxy.https", "host", &ip]);
        let _ = run_cmd("gsettings", &["set", "org.gnome.system.proxy.https", "port", &port]);
        let _ = run_cmd("gsettings", &["set", "org.gnome.system.proxy.socks", "host", &ip]);
        let _ = run_cmd("gsettings", &["set", "org.gnome.system.proxy.socks", "port", &port]);
        log::info!("[system-proxy] set via gsettings (GNOME)");
    }

    // Try KDE (kwriteconfig5)
    if std::process::Command::new("kwriteconfig5").arg("--help").output().is_ok() {
        let group = "KDE";
        let _ = run_cmd("kwriteconfig5", &[group, "HttpProxyEnabled", "true"]);
        let _ = run_cmd("kwriteconfig5", &[group, "HttpProxy", &ip]);
        let _ = run_cmd("kwriteconfig5", &[group, "HttpPort", &port]);
        let _ = run_cmd("kwriteconfig5", &[group, "HttpsProxyEnabled", "true"]);
        let _ = run_cmd("kwriteconfig5", &[group, "HttpsProxy", &ip]);
        let _ = run_cmd("kwriteconfig5", &[group, "HttpsPort", &port]);
        let _ = run_cmd("kwriteconfig5", &[group, "SocksProxyEnabled", "true"]);
        let _ = run_cmd("kwriteconfig5", &[group, "SocksProxy", &ip]);
        let _ = run_cmd("kwriteconfig5", &[group, "SocksPort", &port]);
        // Signal KDE to re-read config
        let _ = run_cmd("dbus-send", &[
            "--session", "--type=signal", "--dest=org.kde.kioslave.net",
            "/KIO/Scheduler", "org.kde.KIOSlave.Scheduler.reparseConfiguration", "string:''"
        ]);
        log::info!("[system-proxy] set via kwriteconfig5 (KDE)");
    }

    // Set env vars for child processes
    let proxy_url = format!("http://{}", addr);
    std::env::set_var("http_proxy", &proxy_url);
    std::env::set_var("https_proxy", &proxy_url);
    std::env::set_var("HTTP_PROXY", &proxy_url);
    std::env::set_var("HTTPS_PROXY", &proxy_url);
    std::env::set_var("ALL_PROXY", &proxy_url);
    log::info!("[system-proxy] env vars set: {proxy_url}");

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

#[cfg(target_os = "linux")]
pub fn clear_proxy() -> Result<(), String> {
    // GNOME
    if std::process::Command::new("gsettings").arg("--version").output().is_ok() {
        let _ = run_cmd("gsettings", &["set", "org.gnome.system.proxy", "mode", "none"]);
        log::info!("[system-proxy] cleared gsettings (GNOME)");
    }

    // KDE
    if std::process::Command::new("kwriteconfig5").arg("--help").output().is_ok() {
        let _ = run_cmd("kwriteconfig5", &["KDE", "HttpProxyEnabled", "false"]);
        let _ = run_cmd("kwriteconfig5", &["KDE", "HttpsProxyEnabled", "false"]);
        let _ = run_cmd("kwriteconfig5", &["KDE", "SocksProxyEnabled", "false"]);
        log::info!("[system-proxy] cleared kwriteconfig5 (KDE)");
    }

    std::env::remove_var("http_proxy");
    std::env::remove_var("https_proxy");
    std::env::remove_var("HTTP_PROXY");
    std::env::remove_var("HTTPS_PROXY");
    std::env::remove_var("ALL_PROXY");
    log::info!("[system-proxy] cleared");
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn set_proxy(addr: SocketAddr) -> Result<(), String> {
    let ip = addr.ip().to_string();
    let port = addr.port().to_string();

    let output = std::process::Command::new("networksetup")
        .args(["-listallnetworkservices"])
        .output()
        .map_err(|e| format!("failed to run networksetup: {e}"))?;
    let services = String::from_utf8_lossy(&output.stdout);
    let service = services.lines().skip(1).next().unwrap_or("Wi-Fi");

    run_cmd("networksetup", &["-setwebproxy", service, &ip, &port])?;
    run_cmd("networksetup", &["-setsecurewebproxy", service, &ip, &port])?;
    run_cmd("networksetup", &["-setsocksfirewallproxy", service, &ip, &port])?;
    log::info!("[system-proxy] set on {service}: {addr}");
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn clear_proxy() -> Result<(), String> {
    let output = std::process::Command::new("networksetup")
        .args(["-listallnetworkservices"])
        .output()
        .map_err(|e| format!("failed to run networksetup: {e}"))?;
    let services = String::from_utf8_lossy(&output.stdout);
    let service = services.lines().skip(1).next().unwrap_or("Wi-Fi");

    run_cmd("networksetup", &["-setwebproxystate", service, "off"])?;
    run_cmd("networksetup", &["-setsecurewebproxystate", service, "off"])?;
    run_cmd("networksetup", &["-setsocksfirewallproxystate", service, "off"])?;
    log::info!("[system-proxy] cleared on {service}");
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn set_proxy(addr: SocketAddr) -> Result<(), String> {
    let proxy_url = format!("http://{}", addr);
    let bypass = "localhost;127.*;10.*;172.16.*;172.17.*;172.18.*;172.19.*;172.20.*;172.21.*;172.22.*;172.23.*;172.24.*;172.25.*;172.26.*;172.27.*;172.28.*;172.29.*;172.30.*;172.31.*;192.168.*;<local>";

    let key = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings";

    // Enable proxy
    run_cmd("reg", &["add", key, "/v", "ProxyEnable", "/t", "REG_DWORD", "/d", "1", "/f"])?;

    // Set proxy server
    run_cmd("reg", &["add", key, "/v", "ProxyServer", "/t", "REG_SZ", "/d", &proxy_url, "/f"])?;

    // Set bypass list
    run_cmd("reg", &["add", key, "/v", "ProxyOverride", "/t", "REG_SZ", "/d", bypass, "/f"])?;

    // Notify the system of the change
    unsafe {
        winapi_refresh_internet_settings();
    }

    log::info!("[system-proxy] set: {proxy_url}");
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn clear_proxy() -> Result<(), String> {
    let key = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings";

    run_cmd("reg", &["add", key, "/v", "ProxyEnable", "/t", "REG_DWORD", "/d", "0", "/f"])?;

    unsafe {
        winapi_refresh_internet_settings();
    }

    log::info!("[system-proxy] cleared");
    Ok(())
}

#[cfg(target_os = "windows")]
unsafe fn winapi_refresh_internet_settings() {
    use std::ffi::c_void;
    extern "system" {
        pub fn InternetSetOptionA(
            hInternet: *mut c_void,
            dwOption: u32,
            lpBuffer: *mut c_void,
            dwBufferLength: u32,
        ) -> i32;
    }
    const INTERNET_OPTION_SETTINGS_CHANGED: u32 = 39;
    const INTERNET_OPTION_REFRESH: u32 = 37;
    let _ = InternetSetOptionA(std::ptr::null_mut(), INTERNET_OPTION_SETTINGS_CHANGED, std::ptr::null_mut(), 0);
    let _ = InternetSetOptionA(std::ptr::null_mut(), INTERNET_OPTION_REFRESH, std::ptr::null_mut(), 0);
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn set_proxy(_addr: SocketAddr) -> Result<(), String> {
    Err("system proxy not supported on this platform".into())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn clear_proxy() -> Result<(), String> {
    Err("system proxy not supported on this platform".into())
}

fn run_cmd(cmd: &str, args: &[&str]) -> Result<(), String> {
    let output = std::process::Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run {cmd}: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() {
            log::warn!("[system-proxy] {cmd} stderr: {stderr}");
        }
    }
    Ok(())
}
