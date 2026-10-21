//! SETTINGS-02C: print the Wi-Fi networks in range (read-only).

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let bus = match zbus::Connection::system().await {
        Ok(bus) => bus,
        Err(error) => {
            eprintln!("GNOS_NETWORK_WIFI_FAIL error={error}");
            std::process::exit(1);
        }
    };
    for n in gnos_network::list_networks(&bus).await {
        println!(
            "GNOS_NETWORK_WIFI ssid={:?} strength={} secured={} saved={} active={}",
            n.ssid, n.strength, n.secured, n.saved, n.active
        );
    }
}
