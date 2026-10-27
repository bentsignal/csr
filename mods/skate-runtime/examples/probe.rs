fn main() -> Result<(),String> {
    let root=std::env::args().nth(1).expect("converted asset root");
    let mut runtime=csr_skate_runtime::Runtime::load(std::path::Path::new(&root))?;
    eprintln!("LOADED {}",runtime.diagnostic());
    eprintln!("BONES {:?}",runtime.rig().names);
    for tick in 0..360 {
        let mut values=[0.;18];
        if tick > 60 && tick < 200 { values[16]=1.; }
        runtime.step(values,true)?;
        if tick % 30 == 0 { eprintln!("{}",runtime.diagnostic()); }
    }
    Ok(())
}
