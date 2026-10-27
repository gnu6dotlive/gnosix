//! One-shot read-only UPower / power-profiles-daemon qualification command.
#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("{:?}", gnos_power::observe_once().await);
}
