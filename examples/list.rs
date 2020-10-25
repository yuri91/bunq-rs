use anyhow::Result;
use bunqledger::BunqConfig;

fn main() -> Result<()> {
    let cfg = BunqConfig::load()?;

    let cfg = cfg.install()?;
    let accs = cfg.monetary_accounts()?;
    println!("{:#?}", accs);
    let acc = &accs[0];
    let ps = cfg.payments(acc)?;
    println!("{:#?}", ps);

    Ok(())
}
