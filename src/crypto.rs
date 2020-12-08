
use rand::rngs::OsRng;
use secp256k1::Secp256k1;
use secp256k1::{PublicKey, SecretKey};
use ethereum_types::H512;
use parity_crypto::publickey::{Generator, KeyPair, Public, Random, recover, Secret, sign, ecdh, ecies};

/// 节点公钥ID
pub type NodeId = H512;


pub fn generate_keypair() -> KeyPair {
    let mut rand = Random{};
    let keypair = rand.generate();
    info!("generate keypair:\n{}", keypair);
    keypair
}

pub fn generate_keypair_from_secret_str(secret: &str) -> Result<KeyPair, Box<dyn std::error::Error>> {
    let s = Secret::copy_from_str(secret)?;
    let keypair = KeyPair::from_secret(s)?;
    info!("generate keypair with secret: {}\n{}", secret, keypair);
    Ok(keypair)
}

/*
node0
secret:  e12aaf3b8500efcd89b7914263067b97a2d81d76f6bbc4b5765611669986b5c6
public:  d122860990af584cc082db2b15d9e388b208f01c7f23366228f140f544daa55352dceccf56f7a71513282a3ee610ff705a2f73ce126950fcf27d9a9d6f784beb
address: 52722c23ef4184f056ce81f2d861089874994164

node1
secret:  52c1e3c6e52d68c56d6987ddfaccd2fc826527f6c84da8000d798c719a026334
public:  39ad8af24e29cef41d0f29f2df33c51506b812176d3eeb808428677721b53f99fe1282c0382deae2143f0627849674fa391fff5edeb17cb6b7faeea52fd2fde9
address: b4db3580f15856cb92ad494b4ea9e3548848cd50

node2
secret:  4d7f48f9dcda78040e510445e7df4a408c43f82beb7e319affcf73e79982fcba
public:  f99d2913b9981e9693e466e416ee95834e58dfa75d5d3c92a60f2684e0ea2df7a223dbfa920e18b04825bc7d71d8405055ce1b0fb3f6db1576b2825c5265adde
address: 814abdae92541dd265dc85ab45c156916ba5e46e
*/