use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls_pemfile::{certs, ec_private_keys, pkcs8_private_keys, rsa_private_keys};

pub(crate) fn load_keys(path: &Path) -> Result<Vec<PrivateKeyDer<'_>>, anyhow::Error> {
  load_ec_keys(path).or_else(|_| load_rsa_pkcks1_keys(path)).or_else(|_| load_pkcs8_keys(path))
}

pub(crate) fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>, anyhow::Error> {
  certs(&mut BufReader::new(File::open(path)?))
    .map(|cert| {
      cert.map_err(|e| anyhow::Error::new(e).context("Could not read certificate".to_string()))
    })
    .collect()
}

pub(crate) fn load_ec_keys(path: &Path) -> Result<Vec<PrivateKeyDer<'static>>, anyhow::Error> {
  ec_private_keys(&mut BufReader::new(File::open(path)?))
    .map(|key| key.map(PrivateKeyDer::Sec1))
    .map(|key| {
      key.map_err(|e| anyhow::Error::new(e).context("Could not read EC certificate".to_string()))
    })
    .collect()
}

pub(crate) fn load_rsa_pkcks1_keys(
  path: &Path,
) -> Result<Vec<PrivateKeyDer<'static>>, anyhow::Error> {
  rsa_private_keys(&mut BufReader::new(File::open(path)?))
    .map(|key| key.map(PrivateKeyDer::Pkcs1))
    .map(|key| {
      key.map_err(|e| anyhow::Error::new(e).context("Could not read RSA certificate".to_string()))
    })
    .collect()
}

pub(crate) fn load_pkcs8_keys(path: &Path) -> Result<Vec<PrivateKeyDer<'static>>, anyhow::Error> {
  pkcs8_private_keys(&mut BufReader::new(File::open(path)?))
    .map(|key| key.map(PrivateKeyDer::Pkcs8))
    .map(|key| {
      key.map_err(|e| anyhow::Error::new(e).context("Could not read PKCS8 certificate".to_string()))
    })
    .collect()
}
