use sha2_const_stable::Sha256;

/// Computes the Anchor account/instruction discriminator: first 8 bytes of SHA256("namespace:name").
pub(crate) const fn anchor_discriminator(namespace: &str, name: &str) -> [u8; 8] {
    let hash = Sha256::new()
        .update(namespace.as_bytes())
        .update(b":")
        .update(name.as_bytes())
        .finalize();
    [
        hash[0], hash[1], hash[2], hash[3], hash[4], hash[5], hash[6], hash[7],
    ]
}
