use std::env;

pub struct Cli {
    pub bind: Option<String>,
    pub mode: Option<String>,
    pub scan: Option<String>,
    pub config: Option<String>,
    pub ip: Option<String>,
    pub noize: Option<String>,
    pub aethernoize: Option<String>,
    pub peer: Option<String>,
    pub ech: Option<String>,
    pub wg_keepalive: Option<u16>,
    pub wg_no_profile_retry: bool,
    pub verbose: bool,
    pub gui: bool,
    pub cli: bool,
    pub tun: bool,
    pub allow_lan: bool,
    pub auth: Option<(String, String)>,
}

impl Cli {
    pub fn parse() -> Self {
        let args: Vec<String> = env::args().skip(1).collect();
        let mut cli = Cli {
            bind: None,
            mode: None,
            scan: None,
            config: None,
            ip: None,
            noize: None,
            aethernoize: None,
            peer: None,
            ech: None,
            wg_keepalive: None,
            wg_no_profile_retry: false,
            verbose: false,
            gui: false,
            cli: false,
            tun: false,
            allow_lan: false,
            auth: None,
        };

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-h" | "--help" => {
                    Self::print_help();
                    std::process::exit(0);
                }
                "-v" | "--verbose" => cli.verbose = true,
                "-b" | "--bind" => { i += 1; cli.bind = args.get(i).cloned(); }
                "-m" | "--mode" => { i += 1; cli.mode = args.get(i).cloned(); }
                "-s" | "--scan" => { i += 1; cli.scan = args.get(i).cloned(); }
                "-c" | "--config" => { i += 1; cli.config = args.get(i).cloned(); }
                "--ip" => { i += 1; cli.ip = args.get(i).cloned(); }
                "--noize" => { i += 1; cli.noize = args.get(i).cloned(); }
                "--aethernoize" => { i += 1; cli.aethernoize = args.get(i).cloned(); }
                "--peer" => { i += 1; cli.peer = args.get(i).cloned(); }
                "--ech" => { i += 1; cli.ech = args.get(i).cloned(); }
                "--wg-keepalive" => {
                    i += 1;
                    cli.wg_keepalive = args.get(i).and_then(|v| v.parse().ok());
                }
                "--wg-no-profile-retry" => cli.wg_no_profile_retry = true,
                "--gui" => cli.gui = true,
                "--cli" => cli.cli = true,
                "--tun" => cli.tun = true,
                "--allow-lan" => cli.allow_lan = true,
                "--auth" => {
                    i += 1;
                    if let Some(val) = args.get(i) {
                        if let Some((u, p)) = val.split_once(':') {
                            cli.auth = Some((u.to_string(), p.to_string()));
                        }
                    }
                }
                other => {
                    eprintln!("error: unknown flag '{other}'");
                    eprintln!("run 'aether --help' for usage");
                    std::process::exit(1);
                }
            }
            i += 1;
        }

        cli
    }

    fn print_help() {
        println!(
            r#"aether {version}

A Cloudflare WARP client with MASQUE and WireGuard support.

USAGE:
    aether [OPTIONS]

OPTIONS:
    -b, --bind <ADDR>           Proxy listen address [default: 127.0.0.1:1819]
    -m, --mode <MODE>           Protocol: masq, wg, gool [default: masq]
    -s, --scan <MODE>           Scan mode: turbo, balanced, thorough, stealth [default: balanced]
    -c, --config <PATH>         Base config file path [default: aether.toml]
        --ip <VERSION>          IP version to scan: v4, v6, both [default: v4]
        --noize <PROFILE>       MASQUE obfuscation: off, gfw, firewall [default: firewall]
        --aethernoize <PROFILE> WG obfuscation: off, light, balanced, aggressive [default: balanced]
        --peer <ADDR>           Force a specific peer address (skips scan)
        --ech <MODE>            ECH: auto, or a base64 ECHConfigList [default: off]
        --wg-keepalive <SECS>   WireGuard persistent keepalive [default: 5]
        --wg-no-profile-retry   Don't retry with fallback aethernoize profiles
        --allow-lan             Bind to 0.0.0.0 (accept connections from LAN)
        --auth <USER:PASS>      Enable proxy authentication
        --tun                   Use TUN device instead of proxy (requires root)
    -v, --verbose               Enable debug logging (RUST_LOG=debug)
        --gui                   Launch the GUI
        --cli                   Launch interactive CLI (default without args is GUI)
    -h, --help                  Print help

ENVIRONMENT VARIABLES (flags take precedence):
    AETHER_SOCKS           Proxy listen address
    AETHER_PROTOCOL        Protocol mode
    AETHER_SCAN            Scan mode
    AETHER_CONFIG          Base config path
    AETHER_IP              IP version to scan
    AETHER_NOIZE           Obfuscation profile
    AETHER_PEER            Force MASQUE peer
    AETHER_WG_PEER         Force WireGuard peer
    AETHER_ECH             ECH config (auto / base64)
    AETHER_WG_KEEPALIVE    WireGuard keepalive seconds
    AETHER_WG_CONFIG       Warp config path
    AETHER_MASQUE_CONFIG   MASQUE config path

EXAMPLES:
    aether                                    # interactive mode
    aether --bind 0.0.0.0:9011 --mode masq --scan turbo
    aether -m wg -s thorough --peer 162.159.193.1:443
    aether --mode gool --verbose
    aether --allow-lan --auth admin:secret     # LAN access with auth
"#,
            version = env!("CARGO_PKG_VERSION")
        );
    }
}
