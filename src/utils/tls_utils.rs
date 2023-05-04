use std::fs::File;
use std::io;
use std::io::{BufReader, ErrorKind};
use std::path::Path;

use rustls::{Certificate, PrivateKey};
use rustls_pemfile::{certs, ec_private_keys, rsa_private_keys};

pub(crate) fn load_keys(path: &Path) -> io::Result<Vec<PrivateKey>> {
  let ec = load_ec_keys(path);
  let rsa = load_rsa_keys(path);

  return match (ec, rsa) {
    (Ok(mut ec_keys), Ok(rsa_keys)) => Ok({
      ec_keys.extend(rsa_keys);
      ec_keys
    }),
    (Ok(ec_keys), Err(_)) => Ok(ec_keys),
    (Err(_), Ok(rsa_keys)) => Ok(rsa_keys),
    (Err(e_ec), Err(_)) => Err(e_ec),
  };
}

pub(crate) fn load_certs(path: &Path) -> io::Result<Vec<Certificate>> {
  certs(&mut BufReader::new(File::open(path)?))
    .map_err(|_| io::Error::new(ErrorKind::InvalidInput, "invalid cert"))
    .map(|certs| certs.into_iter().map(Certificate).collect())
}

pub(crate) fn load_ec_keys(path: &Path) -> io::Result<Vec<PrivateKey>> {
  ec_private_keys(&mut BufReader::new(File::open(path)?))
    .map_err(|_| io::Error::new(ErrorKind::InvalidInput, "invalid ec key"))
    .map(|keys| keys.into_iter().map(PrivateKey).collect())
}

pub(crate) fn load_rsa_keys(path: &Path) -> io::Result<Vec<PrivateKey>> {
  rsa_private_keys(&mut BufReader::new(File::open(path)?))
    .map_err(|_| io::Error::new(ErrorKind::InvalidInput, "invalid rsa key"))
    .map(|keys| keys.into_iter().map(PrivateKey).collect())
}
