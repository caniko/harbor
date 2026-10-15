fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _tls = openssl::ssl::SslContextBuilder::new(openssl::ssl::SslMethod::tls())?.build();
    let digest = openssl::hash::hash(openssl::hash::MessageDigest::sha256(), b"abc")?;
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(
        hex,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    println!("HARBOR_MUSL_OPENSSL_OK");
    Ok(())
}
