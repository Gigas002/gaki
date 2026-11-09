use super::*;
use rsa::PrivateKey;
use rsa::pkcs1::DecodeRsaPrivateKey;

// Throwaway 1024-bit key generated for these tests only.
const TEST_KEY_PEM: &str = "\
-----BEGIN RSA PRIVATE KEY-----
MIICXgIBAAKBgQDbC3LIljUE1e++GKzVPvHVi+edR3siGRlk7UuwtK4cp4mhAX8R
ML/8WquJRG4eEVMxf5eixVaL6SqT2spm3LBBGUpK+0dq8C+K2ySP6pa6mZT836a/
BpEpDzCgcgtd4nvh6Eyg6zsXbL/yOKbqW1ehDfB6pzwBM/o8FlGGBuWVvwIDAQAB
AoGBAMZ7nDoIaeiE1uXhRBNDIy3GxUxuLjRam/kpUPHA9xg98+cyDOsNypmnq1IJ
8OJL/p+Wp1e2C1NiJ9OsaMVGamoVdX6Sztkkhzt+8OwM+2ZV6n8dtLWLdiIVjV1R
DaIBgUitl+88LMet6UbqlyTgHWdSEE/GQwwKvqFP7qgbCM/5AkEA8kUrUHBx2xDi
ZXgvE5Y88ZXjPbrLMHVfkydL/oYE/B8NwfJg0sNVpW99dsDLZPSUj255/F8+lK9e
1nAPNSdO+wJBAOd1U5GW2Wi21dopbo2XKHjwn8fz6d5ydog5wRdw+DhCSlHHBcYJ
HX9jTLBI1UnygOTQh6EPa4oi3RVS+RlTSQ0CQQDn966XYHLDjBEZks+Qc4N61Uc1
bcrrZRbT8QZkd7ICLUGg+BAY/Yrd0jhRIhwdV8l+5ArMx+1Fs/ycWcNV44NNAkAv
adBZpY4ZrkJ2c0BLTuiihaAlhKimwcnYNASC+chPNf0lbWCYZaueOcgu4GyYvk17
EeCGtLLx1Bx3zsxqTK5NAkEAz+Ty3lz5jrDGL+taVZ7pCbgqTprfSYFubnFi82FS
Gk0ikjGmt8qFEyBjyXfMIv1uAvYqADMYS7/p7hlyypYSdw==
-----END RSA PRIVATE KEY-----
";
const TEST_MODULUS: &str = "DB0B72C8963504D5EFBE18ACD53EF1D58BE79D477B22191964ED4BB0B4AE1CA789A1017F1130BFFC5AAB89446E1E1153317F97A2C5568BE92A93DACA66DCB041194A4AFB476AF02F8ADB248FEA96BA9994FCDFA6BF0691290F30A0720B5DE27BE1E84CA0EB3B176CBFF238A6EA5B57A10DF07AA73C0133FA3C16518606E595BF";
const TEST_EXPONENT: &str = "010001";

fn private_key() -> PrivateKey {
    PrivateKey::from_pkcs1_pem(TEST_KEY_PEM).unwrap()
}

#[test]
fn encrypted_password_decrypts_with_the_private_key() {
    let key = PasswordKey::from_hex(TEST_MODULUS, TEST_EXPONENT).unwrap();
    let ciphertext = key.encrypt(b"hunter2").unwrap();
    assert_eq!(ciphertext.len(), 128);
    let plain = private_key().decrypt(Pkcs1v15Encrypt, &ciphertext).unwrap();
    assert_eq!(plain, b"hunter2");
}

#[test]
fn encryption_is_randomised() {
    let key = PasswordKey::from_hex(TEST_MODULUS, TEST_EXPONENT).unwrap();
    assert_ne!(key.encrypt(b"pw").unwrap(), key.encrypt(b"pw").unwrap());
}

#[test]
fn hex_is_case_insensitive_and_odd_lengths_pad() {
    assert!(PasswordKey::from_hex(&TEST_MODULUS.to_lowercase(), "10001").is_ok());
}

#[test]
fn bad_keys_are_rejected() {
    assert!(PasswordKey::from_hex("", TEST_EXPONENT).is_err());
    assert!(PasswordKey::from_hex("zz", TEST_EXPONENT).is_err());
    // A modulus too small for any real key.
    assert!(PasswordKey::from_hex("0F", TEST_EXPONENT).is_err());
}

#[test]
fn too_long_input_is_an_error_not_a_panic() {
    let key = PasswordKey::from_hex(TEST_MODULUS, TEST_EXPONENT).unwrap();
    assert!(key.encrypt(&[b'x'; 200]).is_err());
}
