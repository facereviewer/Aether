use std::net::SocketAddr;

#[cfg(target_os = "linux")]
pub fn set_proxy(addr: SocketAddr) -> Result<(), String> {
    let proxy_url = format!("http://{}", addr);

    // Try gsettings (GNOME)
    let _ = std::process::Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy", "mode", "manual"])
        .output();
    let _ = std::process::Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy.http", "host", &addr.ip().to_string()])
        .output();
    let _ = std::process::Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy.http", "port", &addr.port().to_string()])
        .output();
    let _ = std::process::Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy.https", "host", &addr.ip().to_string()])
        .output();
    let _ = std::process::Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy.https", "port", &addr.port().to_string()])
        .output();
    let _ = std::process::Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy.socks", "host", &addr.ip().to_string()])
        .output();
    let _ = std::process::Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy.socks", "port", &addr.port().to_string()])
        .output();

    // Set env vars for current session and shell profiles
    std::env::set_var("http_proxy", &proxy_url);
    std::env::set_var("https_proxy", &proxy_url);
    std::env::set_var("HTTP_PROXY", &proxy_url);
    std::env::set_var("HTTPS_PROXY", &proxy_url);
    std::env::set_var("ALL_PROXY", &proxy_url);

    log::info!("[system-proxy] set: {proxy_url}");
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn clear_proxy() -> Result<(), String> {
    let _ = std::process::Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy", "mode", "none"])
        .output();

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
    let http_proxy = format!("{}", addr.ip());
    let port = addr.port().to_string();

    // Get active network service
    let output = std::process::Command::new("networksetup")
        .args(["-listallnetworkservices"])
        .output()
        .map_err(|e| e.to_string())?;
    let services = String::from_utf8_lossy(&output.stdout);
    let service = services.lines().skip(1).next().unwrap_or("Wi-Fi");

    let _ = std::process::Command::new("networksetup")
        .args(["-setwebproxy", service, &http_proxy, &port])
        .output();
    let _ = std::process::Command::new("networksetup")
        .args(["-setsecurewebproxy", service, &http_proxy, &port])
        .output();
    let _ = std::process::Command::new("networksetup")
        .args(["-setsocksfirewallproxy", service, &http_proxy, &port])
        .output();

    log::info!("[system-proxy] set on {service}: {addr}");
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn clear_proxy() -> Result<(), String> {
    let output = std::process::Command::new("networksetup")
        .args(["-listallnetworkservices"])
        .output()
        .map_err(|e| e.to_string())?;
    let services = String::from_utf8_lossy(&output.stdout);
    let service = services.lines().skip(1).next().unwrap_or("Wi-Fi");

    let _ = std::process::Command::new("networksetup")
        .args(["-setwebproxystate", service, "off"])
        .output();
    let _ = std::process::Command::new("networksetup")
        .args(["-setsecurewebproxystate", service, "off"])
        .output();
    let _ = std::process::Command::new("networksetup")
        .args(["-setsocksfirewallproxystate", service, "off"])
        .output();

    log::info!("[system-proxy] cleared on {service}");
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn set_proxy(addr: SocketAddr) -> Result<(), String> {
    let proxy_url = format!("http://{}", addr);

    // Set via netsh for current user
    let _ = std::process::Command::new("reg")
        .args([
            "add",
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
            "/v",
            "ProxyEnable",
            "/t",
            "REG_DWORD",
            "/d",
            "1",
            "/f",
        ])
        .output();
    let _ = std::process::Command::new("reg")
        .args([
            "add",
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
            "/v",
            "ProxyServer",
            "/t",
            "REG_SZ",
            "/d",
            &proxy_url,
            "/f",
        ])
        .output();

    log::info!("[system-proxy] set: {proxy_url}");
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn clear_proxy() -> Result<(), String> {
    let _ = std::process::Command::new("reg")
        .args([
            "add",
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
            "/v",
            "ProxyEnable",
            "/t",
            "REG_DWORD",
            "/d",
            "0",
            "/f",
        ])
        .output();

    log::info!("[system-proxy] cleared");
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn set_proxy(_addr: SocketAddr) -> Result<(), String> {
    Err("system proxy not supported on this platform".into())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn clear_proxy() -> Result<(), String> {
    Err("system proxy not supported on this platform".into())
}
