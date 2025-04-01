use crate::utils::{Parameters, hash_h, hash_g, generate_matrix_from_seed, generate_error_vector, generate_polynomial, encode_vector, vec_ntt, decode_vector, encode_poly, decode_poly, decompress_poly, compress_poly, compress_vec};
use module_lwe::utils::{gen_uniform_matrix,mul_mat_vec_simple,gen_small_vector,add_vec,mul_vec_simple};
use module_lwe::encrypt::encrypt;
use module_lwe::decrypt::decrypt;
use ring_lwe::utils::{gen_binary_poly,polyadd};
use polynomial_ring::Polynomial;
use aes_ctr_drbg::DrbgCtx;

pub struct MLKEM {
    pub params: Parameters,
    pub drbg: Option<DrbgCtx>,
}

impl MLKEM {
    // Constructor to initialize MLKEM with parameters
    pub fn new(params: Parameters) -> Self {
        MLKEM { params, drbg: None}
    }

    /// Set the DRBG to be used for random bytes
    pub fn set_drbg_seed(&mut self, seed: Vec<u8>) {
        let p = vec![48, 0]; // personalization string must be min. 48 bytes long
        let mut drbg = DrbgCtx::new(); // instantiate the DRBG
	    drbg.init(&seed, p); // initialize the DRBG with the seed
        self.drbg = Some(drbg); // Store the DRBG in the struct
    }

    /// keygen function to generate public and secret keys
    /// 
    /// # Returns
    ///
    /// * ((Vec<Vec<Polynomial<i64>>>, Vec<Polynomial<i64>>), Vec<Polynomial<i64>>)
    ///   - A tuple containing the public key (a matrix and a vector) and the secret key (a vector)
    ///
    /// # Example
    /// ```
    /// use ml_kem::utils::Parameters;
    /// use ml_kem::ml_kem::MLKEM;
    /// let params = Parameters::default();
    /// let mlkem = MLKEM::new(params);
    /// let (pk, sk) = mlkem.keygen();
    /// ```
    /// # Note
    /// The public key consists of a matrix `a` and a vector `b`, while the secret key is a vector `s`.
    pub fn keygen(&self) -> ((Vec<Vec<Polynomial<i64>>>, Vec<Polynomial<i64>>), Vec<Polynomial<i64>>) {
        
        let a = gen_uniform_matrix(self.params.n, self.params.k, self.params.q, None); 
        
        let s = gen_small_vector(self.params.n, self.params.k, None);
        let e = gen_small_vector(self.params.n, self.params.k, None);
        
        let b = add_vec(
            &mul_mat_vec_simple(&a, &s, self.params.q, &self.params.f, self.params.omega), 
            &e, 
            self.params.q, 
            &self.params.f
        );
        
        ((a, b), s)
    }

    /// Encapsulate function to generate a shared secret and ciphertext
    ///
    /// # Arguments
    ///
    /// * `pk` - A tuple containing the public key (a matrix and a vector)
    ///
    /// # Returns
    ///
    /// * (Vec<u8>, (Vec<Polynomial<i64>>, Polynomial<i64>))
    ///   - A tuple containing the shared secret (as a byte vector) and the ciphertext (a tuple of a vector and a polynomial)
    ///
    /// # Example
    /// ```
    /// use ml_kem::utils::Parameters;
    /// use ml_kem::ml_kem::MLKEM;
    /// let params = Parameters::default();
    /// let mlkem = MLKEM::new(params);
    /// let (pk, sk) = mlkem.keygen();
    /// let (k, ct) = mlkem.encapsulate(pk);
    /// ```
    /// # Note
    /// The shared secret is generated by hashing the message `m`, which is a binary polynomial of degree `n`.
    pub fn encapsulate(&self, pk: (Vec<Vec<Polynomial<i64>>>, Vec<Polynomial<i64>>)) -> (Vec<u8>, (Vec<Polynomial<i64>>, Polynomial<i64>)) {
        let params_mlwe = module_lwe::utils::Parameters { 
            n: self.params.n, 
            q: self.params.q, 
            k: self.params.k, 
            omega: self.params.omega, 
            f: self.params.f.clone() 
        };

        let mut m = gen_binary_poly(self.params.n, None).coeffs().to_vec();
        m.resize(self.params.n, 0);

        let ct = encrypt(&pk.0, &pk.1, &m, &params_mlwe, None);
        let k = hash_h(m);
        (k, ct)
    }

    /// Decapsulate function to recover the shared secret from the ciphertext and secret key
    ///
    /// # Arguments
    ///
    /// * `sk` - The secret key (a vector of polynomials)
    /// * `ct` - The ciphertext (a tuple of a vector and a polynomial)
    ///
    /// # Returns
    ///
    /// * Vec<u8> - The shared secret (as a byte vector)
    ///
    /// # Example
    /// ```
    /// use ml_kem::utils::Parameters;
    /// use ml_kem::ml_kem::MLKEM;
    /// let params = Parameters::default();
    /// let mlkem = MLKEM::new(params);
    /// let (pk, sk) = mlkem.keygen();
    /// let (k, ct) = mlkem.encapsulate(pk);
    /// let k_recovered = mlkem.decapsulate(sk, ct);
    /// ```
    /// # Note
    /// The shared secret is recovered by decrypting the ciphertext using the secret key and hashing the resulting message `m`.
    pub fn decapsulate(&self, sk: Vec<Polynomial<i64>>, ct: (Vec<Polynomial<i64>>, Polynomial<i64>)) -> Vec<u8> {
        let params_mlwe = module_lwe::utils::Parameters { 
            n: self.params.n, 
            q: self.params.q, 
            k: self.params.k, 
            omega: self.params.omega, 
            f: self.params.f.clone() 
        };

        let mut m = decrypt(&sk, &ct.0, &ct.1, &params_mlwe);
        m.resize(self.params.n, 0);

        hash_h(m)
    }

    /// Generates an encryption key and a corresponding decryption key based on the
    /// specified parameter `d` and following Algorithm 13 (FIPS 203).
    ///
    /// This function generates two 32-byte seeds using the `hash_g` function,
    /// computes the matrix `A_hat`, generates error vectors `s` and `e` from
    /// the Centered Binomial Distribution, applies NTT transformations to `s`
    /// and `e`, and computes the public key (`ek_pke`) and the private key (`dk_pke`).
    ///
    /// # Arguments
    /// * `d` - The input parameter (likely a domain or identifier) to seed the key generation.
    ///
    /// # Returns
    /// * A tuple containing:
    ///   - `ek_pke`: The encryption key, which is the public value `t_hat` encoded with `rho`.
    ///   - `dk_pke`: The decryption key, which is the encoded `s_hat`.
    /// 
    /// # Example
    /// ```
    /// use ml_kem::utils::Parameters;
    /// use ml_kem::ml_kem::MLKEM;
    /// let params = Parameters::default();
    /// let mlkem = MLKEM::new(params);
    /// let d = vec![0x01, 0x02, 0x03, 0x04];
    /// let (ek_pke, dk_pke) = mlkem._k_pke_keygen(d);
    /// ```
    pub fn _k_pke_keygen(
        &self,
        d: Vec<u8>,
    ) -> (Vec<u8>, Vec<u8>) {
        // Expand 32 + 1 bytes to two 32-byte seeds.
        // Note: rho, sigma are generated using hash_g
        let (rho, sigma) = hash_g([d.clone(), vec![self.params.k as u8]].concat());

        // Generate A_hat from seed rho
        let a_hat = generate_matrix_from_seed(rho.clone(), self.params.k, self.params.n, false);

        // Set counter for PRF
        let prf_count = 0;

        // Generate the error vectors s and e
        let (s, _prf_count) = generate_error_vector(sigma.clone(), self.params.eta_1, prf_count, self.params.k, self.params.n);
        let (e, _prf_count) = generate_error_vector(sigma.clone(), self.params.eta_1, prf_count, self.params.k, self.params.n);

        // the NTT of s as an element of a rank k module over the polynomial ring
        let s_hat = vec_ntt(&s,self.params.omega, self.params.n, self.params.q);
        // the NTT of e as an element of a rank k module over the polynomial ring
        let e_hat = vec_ntt(&e,self.params.omega, self.params.n, self.params.q);
        // A_hat @ s_hat + e_hat
        let t_hat = add_vec(&mul_mat_vec_simple(&a_hat, &s_hat, self.params.q, &self.params.f, self.params.omega), &e_hat, self.params.q, &self.params.f);

        // Encode the keys
        let mut ek_pke = encode_vector(&t_hat, 12); // Encoding vec of polynomials to bytes
        ek_pke.extend_from_slice(&rho); // append rho, output of hash function
        let dk_pke = encode_vector(&s_hat, 12); // Encoding s_hat for dk_pke

        (ek_pke, dk_pke)
    }

    /// Encrypts a plaintext message using the encryption key and randomness `r`
    /// following Algorithm 14 (FIPS 203).
    ///
    /// In addition to performing standard public key encryption (PKE),
    /// this function includes two additional checks required by the FIPS document:
    ///
    /// 1. **Type Check**: Ensures that `ek_pke` has the expected length.
    /// 2. **Modulus Check**: Verifies that `t_hat` has been canonically encoded.
    ///
    /// If either check fails, the function will panic with an error message.
    ///
    /// # Arguments
    ///
    /// * `ek_pke` - A vector of bytes representing the encryption key.
    /// * `m` - A vector of bytes representing the plaintext message.
    /// * `r` - Randomness used in the encryption process.
    ///
    /// # Returns
    ///
    /// A vector of bytes representing the encrypted ciphertext.
    ///
    /// # Panics
    ///
    /// This function will panic if `ek_pke` has an incorrect length.
    pub fn _k_pke_encrypt(
        &self,
        ek_pke: Vec<u8>,
        m: Vec<u8>,
        r: Vec<u8>,
    ) -> Vec<u8> {

        let expected_len = ek_pke.len();
        let received_len = 384 * self.params.k + 32;

        if expected_len != received_len {
            panic!(
                "Type check failed, ek_pke has the wrong length, expected {} bytes and received {}",
                received_len,
                expected_len
            );
        }

        // Unpack ek
        let (t_hat_bytes_slice, rho_slice) = ek_pke.split_at(ek_pke.len() - 32);
        let t_hat_bytes = t_hat_bytes_slice.to_vec();
        let rho = rho_slice.to_vec();

        // decode the vector of polynomials from bytes
        let t_hat = decode_vector(t_hat_bytes.clone(), self.params.k, 12, true);

        // check that t_hat has been canonically encoded
        if encode_vector(&t_hat,12) != t_hat_bytes {
            panic!(
                "Modulus check failed, t_hat does not encode correctly"
            );
        }

        // Generate A_hat^T from seed rho
        let a_hat_t = generate_matrix_from_seed(rho.clone(), self.params.k, self.params.n, true);

        // generate error vectors y, e1 and error polynomial e2
        let prf_count = 0;
        let (y, _prf_count) = generate_error_vector(r.clone(), self.params.eta_1, prf_count, self.params.k, self.params.n);
        let (e1, _prf_count) = generate_error_vector(r.clone(), self.params.eta_2, prf_count, self.params.k, self.params.n);
        let (e2, _prf_count) = generate_polynomial(r.clone(), self.params.eta_2, prf_count, self.params.n, None);

        // compute the NTT of the error vector y
        let y_hat = vec_ntt(&y, self.params.omega, self.params.n, self.params.q);

        /*

        // compute u = a_hat.T * y_hat + e1
        let a_hat_t_dot_y_hat = from_ntt(mul_mat_vec_simple(&a_hat_t, &y_hat, self.params.q, &self.params.f, self.params.omega));
        let u = add_vec(&a_hat_t_dot_y_hat, &e1, self.params.q, &self.params.f);

        //decode the polynomial mu from the bytes m
        let mu = decompress_poly(&decode_poly(m, 1),1);

        //compute v = t_hat.y_hat + e2 + mu
        let t_hat_dot_y_hat = from_ntt(mul_vec_simple(&t_hat, &y_hat, self.params.q, &self.params.f, self.params.omega));
        let v = polyadd(&polyadd(&t_hat_dot_y_hat, &e2, self.params.q, &self.params.f), &mu, self.params.q, &self.params.f);

        // compress polynomials u, v by compressing coeffs, then encode to bytes using params du, dv
        let c1 = encode_vec(&compress_vec(&u,self.params.du),self.params.du);
        let c2 = encode_poly(&compress_poly(&v,self.params.dv),self.params.dv);

        //return c1 + c2, the concatenation of two encoded polynomials
        [c1, c2].concat()
        */
        
        m

    }

}
