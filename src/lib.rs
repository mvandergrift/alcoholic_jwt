//! Implements a library for verifying JSON Web Tokens using the
//! `RS256` signature algorithm.
//!
//! This library is specifically aimed at developers that consume
//! tokens from services which provide their RSA public keys in
//! [JWKS][] format.
//!
//! ## Usage example (token with `kid`-claim)
//!
//! ```rust
//! # extern crate serde_json;
//! extern crate alcoholic_jwt;
//!
//! use alcoholic_jwt::{JWKS, Validation, validate, token_kid};
//!
//! # fn some_token_fetching_function() -> String {
//! #   "eyJraWQiOiI4ckRxOFB3MEZaY2FvWFdURVZRbzcrVGYyWXpTTDFmQnhOS1BDZWJhYWk0PSIsImFsZyI6IlJTMjU2IiwidHlwIjoiSldUIn0.eyJpc3MiOiJhdXRoLnRlc3QuYXByaWxhLm5vIiwiaWF0IjoxNTM2MDUwNjkzLCJleHAiOjE1MzYwNTQyOTMsInN1YiI6IjQyIiwiZXh0Ijoic21va2V0ZXN0IiwicHJ2IjoiYXJpc3RpIiwic2NwIjoicHJvY2VzcyJ9.gOLsv98109qLkmRK6Dn7WWRHLW7o8W78WZcWvFZoxPLzVO0qvRXXRLYc9h5chpfvcWreLZ4f1cOdvxv31_qnCRSQQPOeQ7r7hj_sPEDzhKjk-q2aoNHaGGJg1vabI--9EFkFsGQfoS7UbMMssS44dgR68XEnKtjn0Vys-Vzbvz_CBSCH6yQhRLik2SU2jR2L7BoFvh4LGZ6EKoQWzm8Z-CHXLGLUs4Hp5aPhF46dGzgAzwlPFW4t9G4DciX1uB4vv1XnfTc5wqJch6ltjKMde1GZwLR757a8dJSBcmGWze3UNE2YH_VLD7NCwH2kkqr3gh8rn7lWKG4AUIYPxsw9CB".into()
//! # }
//!
//! # fn jwks_fetching_function() -> JWKS {
//! #   let jwks_json = "{\"keys\":[{\"kty\":\"RSA\",\"alg\":\"RS256\",\"use\":\"sig\",\"kid\":\"8rDq8Pw0FZcaoXWTEVQo7+Tf2YzSL1fBxNKPCebaai4=\",\"n\":\"l4UTgk1zr-8C8utt0E57DtBV6qqAPWzVRrIuQS2j0_hp2CviaNl5XzGRDnB8gwk0Hx95YOhJupAe6RNq5ok3fDdxL7DLvppJNRLz3Ag9CsmDLcbXgNEQys33fBJaPw1v3GcaFC4tisU5p-o1f5RfWwvwdBtdBfGiwT1GRvbc5sFx6M4iYjg9uv1lNKW60PqSJW4iDYrfqzZmB0zF1SJ0BL_rnQZ1Wi_UkFmNe9arM8W9tI9T3Ie59HITFuyVSTCt6qQEtSfa1e5PiBaVuV3qoFI2jPBiVZQ6LPGBWEDyz4QtrHLdECPPoTF30NN6TSVwwlRbCuUUrdNdXdjYe2dMFQ\",\"e\":\"DhaD5zC7mzaDvHO192wKT_9sfsVmdy8w8T8C9VG17_b1jG2srd3cmc6Ycw-0blDf53Wrpi9-KGZXKHX6_uIuJK249WhkP7N1SHrTJxO0sUJ8AhK482PLF09Qtu6cUfJqY1X1y1S2vACJZItU4Vjr3YAfiVGQXeA8frAf7Sm4O1CBStCyg6yCcIbGojII0jfh2vSB-GD9ok1F69Nmk-R-bClyqMCV_Oq-5a0gqClVS8pDyGYMgKTww2RHgZaFSUcG13KeLMQsG2UOB2OjSC8FkOXK00NBlAjU3d0Vv-IamaLIszO7FQBY3Oh0uxNOvIE9ofQyCOpB-xIK6V9CTTphxw\"}]}";
//! #   serde_json::from_str(jwks_json).unwrap()
//! # }
//!
//!
//! // The function implied here would usually perform an HTTP-GET
//! // on the JWKS-URL for an authentication provider and deserialize
//! // the result into the `alcoholic_jwt::JWKS`-struct.
//! let jwks: JWKS = jwks_fetching_function();
//!
//! let token: String = some_token_fetching_function();
//!
//! // Several types of built-in validations are provided:
//! let validations = vec![
//!   Validation::Issuer("some-issuer".into()),
//!   Validation::Audience("some-audience".into()),
//!   Validation::SubjectPresent,
//! ];
//!
//! // If a JWKS contains multiple keys, the correct KID first
//! // needs to be fetched from the token headers.
//! let kid = token_kid(&token)
//!     .expect("Failed to decode token headers")
//!     .expect("No 'kid' claim present in token");
//!
//! let jwk = jwks.find(&kid).expect("Specified key not found in set");
//!
//! validate(token, jwk, validations).expect("Token validation has failed!");
//! ```
//!
//! [JWKS]: https://tools.ietf.org/html/rfc7517

#[macro_use] extern crate serde_derive;

extern crate base64;
extern crate openssl;
extern crate serde;
extern crate serde_json;

use base64::{decode_config, URL_SAFE};
use openssl::bn::BigNum;
use openssl::error::ErrorStack;
use openssl::hash::MessageDigest;
use openssl::pkey::{Public, PKey};
use openssl::rsa::Rsa;
use openssl::sign::Verifier;
use serde::de::DeserializeOwned;
use serde_json::Value;

#[cfg(test)]
mod tests;

/// JWT algorithm used. The only supported algorithm is currently
/// RS256.
#[derive(Deserialize, Debug)]
enum KeyAlgorithm { RS256 }

/// Type of key contained in a JWT. The only supported key type is
/// currently RSA.
#[derive(Deserialize, Debug)]
enum KeyType { RSA }

/// Representation of a single JSON Web Key. See [RFC
/// 7517](https://tools.ietf.org/html/rfc7517#section-4).
#[allow(dead_code)] // kty & alg only constrain deserialisation, but aren't used
#[derive(Deserialize)]
pub struct JWK {
    kty: KeyType,
    alg: Option<KeyAlgorithm>,
    kid: Option<String>,

    // Shared modulus
    n: String,

    // Public key exponent
    e: String,
}

/// Representation of a set of JSON Web Keys. See [RFC
/// 7517](https://tools.ietf.org/html/rfc7517#section-5).
#[derive(Deserialize)]
pub struct JWKS {
    // This is a vector instead of some kind of map-like structure
    // because key IDs are in fact optional.
    //
    // Technically having multiple keys with the same KID would not
    // violate the JWKS-definition either, but behaviour in that case
    // is unspecified.
    keys: Vec<JWK>,
}

impl JWKS {
    /// Attempt to find a JWK by its key ID.
    pub fn find(&self, kid: &str) -> Option<&JWK> {
        self.keys.iter().find(|jwk| jwk.kid == Some(kid.into()))
    }
}

/// Representation of an undecoded JSON Web Token. See [RFC
/// 7519](https://tools.ietf.org/html/rfc7519).
struct JWT (String);

/// Representation of a decoded and validated JSON Web Token.
///
/// Specific claim fields are only decoded internally in the library
/// for validation purposes, while it is generally up to the consumer
/// of the validated JWT what structure they would like to impose.
pub struct ValidJWT {
    /// JOSE header of the JSON Web Token. Certain fields are
    /// guaranteed to be present in this header, consult section 5 of
    /// RFC7519 for more information.
    pub headers: Value,

    /// Claims (i.e. primary data) contained in the JSON Web Token.
    /// While there are several registered and recommended headers
    /// (consult section 4.1 of RFC7519), the presence of no field is
    /// guaranteed in these.
    pub claims: Value,
}

/// Possible token claim validations. This enumeration only covers
/// common use-cases, for other types of validations the user is
/// encouraged to inspect the claim set manually.
pub enum Validation {
    /// Validate that the issuer ("iss") claim matches a specified
    /// value.
    Issuer(String),

    /// Validate that the audience ("aud") claim matches a specified
    /// value.
    Audience(String),

    /// Validate that a subject value is present.
    SubjectPresent,
}

/// Possible results of a token validation.
#[derive(Debug)]
pub enum ValidationError {
    /// Token was malformed (various possible reasons!)
    MalformedJWT,

    /// Decoding of the provided JWK failed.
    InvalidJWK,

    /// Signature validation failed, i.e. because of a non-matching
    /// public key.
    InvalidSignature,

    /// An OpenSSL operation failed along the way at a point at which
    /// a more specific error variant could not be constructed.
    OpenSSL(ErrorStack),

    /// JSON decoding into a provided type failed.
    JSON(serde_json::Error),

    /// One or more claim validations failed.
    // TODO: Provide reasons?
    InvalidClaims,
}

type JWTResult<T> = Result<T, ValidationError>;

impl From<ErrorStack> for ValidationError {
    fn from(err: ErrorStack) -> Self { ValidationError::OpenSSL(err) }
}

impl From<serde_json::Error> for ValidationError {
    fn from(err: serde_json::Error) -> Self { ValidationError::JSON(err) }
}

/// Attempt to extract the `kid`-claim out of a JWT's header claims.
///
/// This function is normally used when a token provider has multiple
/// public keys in rotation at the same time that could all still have
/// valid tokens issued under them.
///
/// This is only safe if the key set containing the currently allowed
/// key IDs is fetched from a trusted source.
pub fn token_kid(token: &str) -> JWTResult<Option<String>> {
    // Fetch the header component of the JWT by splitting it out and
    // dismissing the rest.
    let parts: Vec<&str> = token.splitn(2, '.').collect();
    if parts.len() != 2 {
        return Err(ValidationError::MalformedJWT);
    }

    // Decode only the first part of the token into a specialised
    // representation:
    #[derive(Deserialize)]
    struct KidOnly {
        kid: Option<String>,
    }

    let kid_only: KidOnly = deserialize_part(parts[0])?;

    Ok(kid_only.kid)
}

/// Validate the signature of a JSON Web Token and optionally apply
/// claim validations. Signatures are always verified before claims,
/// and if a signature verification passes *all* claim validations are
/// run and returned.
///
/// If validation succeeds a representation of the token is returned
/// that contains the header and claims as simple JSON values.
///
/// It is the user's task to ensure that the correct JWK is passed in
/// for validation.
pub fn validate(token: String,
                jwk: &JWK,
                validations: Vec<Validation>) -> JWTResult<ValidJWT> {
    let jwt = JWT(token);
    let public_key = public_key_from_jwk(&jwk)?;
    validate_jwt_signature(&jwt, public_key)?;

    // Split out all three parts of the JWT this time, deserialising
    // the first and second as appropriate.
    let parts: Vec<&str> = jwt.0.splitn(3, '.').collect();
    if parts.len() != 3 {
        // This is unlikely considering that validation has already
        // been performed at this point, but better safe than sorry.
        return Err(ValidationError::MalformedJWT)
    }

    let headers = deserialize_part(parts[0])?;
    let claims = deserialize_part(parts[1])?;
    let valid_jwt = ValidJWT { headers, claims };

    Ok(valid_jwt)
}

// Internal implementation
//
// The functions in the following section are not part of the public
// API of this library.

/// Decode a single key fragment (base64-url encoded integer) to an
/// OpenSSL BigNum.
fn decode_fragment(fragment: &str) -> JWTResult<BigNum> {
    let bytes = decode_config(fragment, URL_SAFE)
        .map_err(|_| ValidationError::InvalidJWK)?;

    BigNum::from_slice(&bytes).map_err(Into::into)
}

/// Decode an RSA public key from a JWK by constructing it directly
/// from the public RSA key fragments.
fn public_key_from_jwk(jwk: &JWK) -> JWTResult<Rsa<Public>> {
    let jwk_n = decode_fragment(&jwk.n)?;
    let jwk_e = decode_fragment(&jwk.e)?;
    Rsa::from_public_components(jwk_n, jwk_e).map_err(Into::into)
}

/// Decode a base64-URL encoded string and deserialise the resulting
/// JSON.
fn deserialize_part<T: DeserializeOwned>(part: &str) -> JWTResult<T> {
    let json = base64::decode_config(part, URL_SAFE)
        .map_err(|_| ValidationError::MalformedJWT)?;

    serde_json::from_slice(&json).map_err(Into::into)
}

/// Validate the signature on a JWT using a provided public key.
///
/// A JWT is made up of three components (headers, claims, signature)
/// - only the first two are part of the signed data.
fn validate_jwt_signature(jwt: &JWT, key: Rsa<Public>) -> JWTResult<()> {
    let key = PKey::from_rsa(key)?;
    let mut verifier = Verifier::new(MessageDigest::sha256(), &key)?;

    // Split the token from the back to a maximum of two elements.
    // There are technically three components using the same separator
    // ('.'), but we are interested in the first two together and
    // splitting them is unnecessary.
    let token_parts: Vec<&str> = jwt.0.rsplitn(2, '.').collect();
    if token_parts.len() != 2 {
        return Err(ValidationError::MalformedJWT);
    }

    // Second element of the vector will be the signed payload.
    let data = token_parts[1];

    // First element of the vector will be the (encoded) signature.
    let sig_b64 = token_parts[0];
    let sig = base64::decode_config(sig_b64, URL_SAFE)
        .map_err(|_| ValidationError::MalformedJWT)?;

    // Verify signature by inserting the payload data and checking it
    // against the decoded signature.
    verifier.update(data.as_bytes())?;

    match verifier.verify(&sig)? {
        true  => Ok(()),
        false => Err(ValidationError::InvalidSignature),
    }
}
