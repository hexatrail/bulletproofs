#![feature(nll)]
#![feature(external_doc)]
#![feature(try_trait)]
#![deny(missing_docs)]
#![doc(include = "../README.md")]
#![doc(html_logo_url = "https://doc.dalek.rs/assets/dalek-logo-clear.png")]

extern crate byteorder;
extern crate core;
extern crate digest;
extern crate rand;
extern crate sha3;

extern crate clear_on_drop;
extern crate curve25519_dalek;
extern crate merlin;
extern crate subtle;

#[macro_use]
extern crate serde_derive;
extern crate serde;

#[macro_use]
extern crate failure;

#[cfg(test)]
extern crate bincode;

mod util;

#[doc(include = "../docs/notes.md")]
mod notes {}
mod circuit_proof;
mod errors;
mod generators;
mod inner_product_proof;
mod range_proof;
mod transcript;

pub use errors::ProofError;
pub use generators::{BulletproofGens, BulletproofGensShare, PedersenGens};
pub use range_proof::RangeProof;

#[doc(include = "../docs/aggregation-api.md")]
pub mod range_proof_mpc {
    pub use errors::MPCError;
    pub use range_proof::dealer;
    pub use range_proof::messages;
    pub use range_proof::party;
}

/// The rank-1 constraint system API for programmatically defining constraint systems.
///
/// XXX explain how the parts fit together here
///
/// ```
/// extern crate bulletproofs;
/// use bulletproofs::r1cs::{Assignment, ConstraintSystem, Variable, ProverCS, VerifierCS, R1CSError};
/// use bulletproofs::{BulletproofGens, PedersenGens};
///
/// extern crate curve25519_dalek;
/// use curve25519_dalek::scalar::Scalar;
///
/// extern crate subtle;
/// use subtle::{ConditionallySelectable, ConstantTimeEq};
///
/// extern crate merlin;
/// use merlin::Transcript;
///
/// #[macro_use]
/// extern crate failure;
/// extern crate rand;
///
/// /*
/// K-SHUFFLE GADGET SPECIFICATION:
///
/// Represents a permutation of a list of `k` scalars `{x_i}` into a list of `k` scalars `{y_i}`.
///
/// Algebraically it can be expressed as a statement that for a free variable `z`,
/// the roots of the two polynomials in terms of `z` are the same up to a permutation:
///
///     ∏(x_i - z) == ∏(y_i - z)
///
/// Prover can commit to blinded scalars `x_i` and `y_i` then receive a random challenge `z`,
/// and build a proof that the above relation holds.
///
/// K-shuffle requires `2*(K-1)` multipliers.
///
/// For K > 1:
///
///         (x_0 - z)---⊗------⊗---(y_0 - z)        // mulx[0], muly[0]
///                     |      |
///         (x_1 - z)---⊗      ⊗---(y_1 - z)        // mulx[1], muly[1]
///                     |      |
///                    ...    ...
///                     |      |
///     (x_{k-2} - z)---⊗      ⊗---(y_{k-2} - z)    // mulx[k-2], muly[k-2]
///                    /        \
///     (x_{k-1} - z)_/          \_(y_{k-1} - z)
///
///     // Connect left and right sides of the shuffle statement
///     mulx_out[0] = muly_out[0]
///
///     // For i == [0, k-3]:
///     mulx_left[i]  = x_i - z
///     mulx_right[i] = mulx_out[i+1]
///     muly_left[i]  = y_i - z
///     muly_right[i] = muly_out[i+1]
///
///     // last multipliers connect two last variables (on each side)
///     mulx_left[k-2]  = x_{k-2} - z
///     mulx_right[k-2] = x_{k-1} - z
///     muly_left[k-2]  = y_{k-2} - z
///     muly_right[k-2] = y_{k-1} - z
///
/// For K = 1:
///
///     (x_0 - z)--------------(y_0 - z)
///
///     // Connect x to y directly, omitting the challenge entirely as it cancels out
///     x_0 = y_0
/// */
///
/// struct KShuffleGadget {}
///
/// impl KShuffleGadget {
///     fn fill_cs<CS: ConstraintSystem>(
///         cs: &mut CS,
///         x: Vec<(Variable, Assignment)>,
///         y: Vec<(Variable, Assignment)>,
///     ) -> Result<(), KShuffleError> {
///         let one = Scalar::one();
///         let z = cs.challenge_scalar(b"k-shuffle challenge");
///         let neg_z = -z;
///
///         if x.len() != y.len() {
///             return Err(KShuffleError::InvalidR1CSConstruction);
///         }
///         let k = x.len();
///         if k == 1 {
///             cs.add_constraint([(x[0].0, -one), (y[0].0, one)].iter().collect());
///             return Ok(());
///         }
///
///         // Make last x multiplier for i = k-1 and k-2
///         let mut mulx_left = x[k - 1].1 + neg_z;
///         let mut mulx_right = x[k - 2].1 + neg_z;
///         let mut mulx_out = mulx_left * mulx_right;
///
///         let mut mulx_out_var_prev = KShuffleGadget::multiplier_helper(
///             cs,
///             neg_z,
///             mulx_left,
///             mulx_right,
///             mulx_out,
///             x[k - 1].0,
///             x[k - 2].0,
///             true,
///         )?;
///
///         // Make multipliers for x from i == [0, k-3]
///         for i in (0..k - 2).rev() {
///             mulx_left = mulx_out;
///             mulx_right = x[i].1 + neg_z;
///             mulx_out = mulx_left * mulx_right;
///
///             mulx_out_var_prev = KShuffleGadget::multiplier_helper(
///                 cs,
///                 neg_z,
///                 mulx_left,
///                 mulx_right,
///                 mulx_out,
///                 mulx_out_var_prev,
///                 x[i].0,
///                 false,
///             )?;
///         }
///
///         // Make last y multiplier for i = k-1 and k-2
///         let mut muly_left = y[k - 1].1 - z;
///         let mut muly_right = y[k - 2].1 - z;
///         let mut muly_out = muly_left * muly_right;
///
///         let mut muly_out_var_prev = KShuffleGadget::multiplier_helper(
///             cs,
///             neg_z,
///             muly_left,
///             muly_right,
///             muly_out,
///             y[k - 1].0,
///             y[k - 2].0,
///             true,
///         )?;
///
///         // Make multipliers for y from i == [0, k-3]
///         for i in (0..k - 2).rev() {
///             muly_left = muly_out;
///             muly_right = y[i].1 + neg_z;
///             muly_out = muly_left * muly_right;
///
///             muly_out_var_prev = KShuffleGadget::multiplier_helper(
///                 cs,
///                 neg_z,
///                 muly_left,
///                 muly_right,
///                 muly_out,
///                 muly_out_var_prev,
///                 y[i].0,
///                 false,
///             )?;
///         }
///
///         // Check equality between last x mul output and last y mul output
///         cs.add_constraint(
///             [(muly_out_var_prev, -one), (mulx_out_var_prev, one)]
///                 .iter()
///                 .collect(),
///         );
///
///         Ok(())
///     }
///
///     fn multiplier_helper<CS: ConstraintSystem>(
///         cs: &mut CS,
///         neg_z: Scalar,
///         left: Assignment,
///         right: Assignment,
///         out: Assignment,
///         left_var: Variable,
///         right_var: Variable,
///         is_last_mul: bool,
///     ) -> Result<Variable, KShuffleError> {
///         let one = Scalar::one();
///         let var_one = Variable::One();
///
///         // Make multiplier gate variables
///         let (left_mul_var, right_mul_var, out_mul_var) = cs.assign_multiplier(left, right, out)?;
///
///         if is_last_mul {
///             // Make last multiplier
///             cs.add_constraint(
///                 [(left_mul_var, -one), (var_one, neg_z), (left_var, one)]
///                     .iter()
///                     .collect(),
///             );
///         } else {
///             // Make intermediate multiplier
///             cs.add_constraint([(left_mul_var, -one), (left_var, one)].iter().collect());
///         }
///         cs.add_constraint(
///             [(right_mul_var, -one), (var_one, neg_z), (right_var, one)]
///                 .iter()
///                 .collect(),
///         );
///
///         Ok(out_mul_var)
///     }
/// }
///
/// /// Represents an error during the proof creation of verification for a KShuffle or KValueShuffle gadget.
/// #[derive(Fail, Copy, Clone, Debug, Eq, PartialEq)]
/// pub enum KShuffleError {
///     /// Error in the constraint system creation process
///     #[fail(display = "Invalid KShuffle constraint system construction")]
///     InvalidR1CSConstruction,
///     /// Occurs when there are insufficient generators for the proof.
///     #[fail(display = "Invalid generators size, too few generators for proof")]
///     InvalidGeneratorsLength,
///     /// Occurs when verification of an [`R1CSProof`](::r1cs::R1CSProof) fails.
///     #[fail(display = "R1CSProof did not verify correctly.")]
///     VerificationError,
/// }
///
/// impl From<R1CSError> for KShuffleError {
///     fn from(e: R1CSError) -> KShuffleError {
///         match e {
///             R1CSError::InvalidGeneratorsLength => KShuffleError::InvalidGeneratorsLength,
///             R1CSError::MissingAssignment => KShuffleError::InvalidR1CSConstruction,
///             R1CSError::VerificationError => KShuffleError::VerificationError,
///         }
///     }
/// }
///
/// // This test allocates variables for the high-level variables, to check that high-level
/// // variable allocation and commitment works.
/// fn shuffle_helper(input: Vec<u64>, output: Vec<u64>) -> Result<(), KShuffleError> {
///     // Common
///     let pc_gens = PedersenGens::default();
///     let bp_gens = BulletproofGens::new(128, 1);
///
///     let k = input.len();
///     if k != output.len() {
///         return Err(KShuffleError::InvalidR1CSConstruction);
///     }
///
///     // Prover's scope
///     let (proof, commitments) = {
///         // Prover makes a `ConstraintSystem` instance representing a shuffle gadget
///         // Make v vector
///         let mut v = vec![];
///         for i in 0..k {
///             v.push(Scalar::from(input[i]));
///         }
///         for i in 0..k {
///             v.push(Scalar::from(output[i]));
///         }
///
///         // Make v_blinding vector using RNG from transcript
///         let mut prover_transcript = Transcript::new(b"ShuffleTest");
///         let mut rng = {
///             let mut builder = prover_transcript.build_rng();
///
///             // commit the secret values
///             for &v_i in &v {
///                 builder = builder.commit_witness_bytes(b"v_i", v_i.as_bytes());
///             }
///
///             use rand::thread_rng;
///             builder.finalize(&mut thread_rng())
///         };
///         let v_blinding: Vec<Scalar> = (0..2 * k).map(|_| Scalar::random(&mut rng)).collect();
///
///         let (mut prover_cs, variables, commitments) = ProverCS::new(
///             &bp_gens,
///             &pc_gens,
///             &mut prover_transcript,
///             v,
///             v_blinding.clone(),
///         );
///
///         // Prover allocates variables and adds constraints to the constraint system
///         let in_pairs = variables[0..k]
///             .iter()
///             .zip(input.iter())
///             .map(|(var_i, in_i)| (*var_i, Assignment::from(in_i.clone())))
///             .collect();
///         let out_pairs = variables[k..2 * k]
///             .iter()
///             .zip(output.iter())
///             .map(|(var_i, out_i)| (*var_i, Assignment::from(out_i.clone())))
///             .collect();
///
///         KShuffleGadget::fill_cs(&mut prover_cs, in_pairs, out_pairs)?;
///         let proof = prover_cs.prove()?;
///
///         (proof, commitments)
///     };
///
///     // Verifier makes a `ConstraintSystem` instance representing a shuffle gadget
///     let mut verifier_transcript = Transcript::new(b"ShuffleTest");
///     let (mut verifier_cs, variables) =
///         VerifierCS::new(&bp_gens, &pc_gens, &mut verifier_transcript, commitments);
///
///     // Verifier allocates variables and adds constraints to the constraint system
///     let in_pairs = variables[0..k]
///         .iter()
///         .map(|var_i| (*var_i, Assignment::Missing()))
///         .collect();
///     let out_pairs = variables[k..2 * k]
///         .iter()
///         .map(|var_i| (*var_i, Assignment::Missing()))
///         .collect();
///     assert!(KShuffleGadget::fill_cs(&mut verifier_cs, in_pairs, out_pairs).is_ok());
///
///     // Verifier verifies proof
///     Ok(verifier_cs.verify(&proof)?)
/// }
///
/// # fn main() {
///     // k=1
///     assert!(shuffle_helper(vec![3], vec![3]).is_ok());
///     assert!(shuffle_helper(vec![6], vec![6]).is_ok());
///     assert!(shuffle_helper(vec![3], vec![6]).is_err());
///     // k=2
///     assert!(shuffle_helper(vec![3, 6], vec![3, 6]).is_ok());
///     assert!(shuffle_helper(vec![3, 6], vec![6, 3]).is_ok());
///     assert!(shuffle_helper(vec![6, 6], vec![6, 6]).is_ok());
///     assert!(shuffle_helper(vec![3, 3], vec![6, 3]).is_err());
///     // k=3
///     assert!(shuffle_helper(vec![3, 6, 10], vec![3, 6, 10]).is_ok());
///     assert!(shuffle_helper(vec![3, 6, 10], vec![3, 10, 6]).is_ok());
///     assert!(shuffle_helper(vec![3, 6, 10], vec![6, 3, 10]).is_ok());
///     assert!(shuffle_helper(vec![3, 6, 10], vec![6, 10, 3]).is_ok());
///     assert!(shuffle_helper(vec![3, 6, 10], vec![10, 3, 6]).is_ok());
///     assert!(shuffle_helper(vec![3, 6, 10], vec![10, 6, 3]).is_ok());
///     assert!(shuffle_helper(vec![3, 6, 10], vec![30, 6, 10]).is_err());
///     assert!(shuffle_helper(vec![3, 6, 10], vec![3, 60, 10]).is_err());
///     assert!(shuffle_helper(vec![3, 6, 10], vec![3, 6, 100]).is_err());
///     // k=4
///     assert!(shuffle_helper(vec![3, 6, 10, 15], vec![3, 6, 10, 15]).is_ok());
///     assert!(shuffle_helper(vec![3, 6, 10, 15], vec![15, 6, 10, 3]).is_ok());
///     assert!(shuffle_helper(vec![3, 6, 10, 15], vec![3, 6, 10, 3]).is_err());
///     // k=5
///     assert!(shuffle_helper(vec![3, 6, 10, 15, 17], vec![3, 6, 10, 15, 17]).is_ok());
///     assert!(shuffle_helper(vec![3, 6, 10, 15, 17], vec![10, 17, 3, 15, 6]).is_ok());
///     assert!(shuffle_helper(vec![3, 6, 10, 15, 17], vec![3, 6, 10, 15, 3]).is_err());
/// # }
///
/// ```

pub mod r1cs {
    pub use circuit_proof::assignment::Assignment;
    pub use circuit_proof::prover::ProverCS;
    pub use circuit_proof::verifier::VerifierCS;
    pub use circuit_proof::ConstraintSystem;
    pub use circuit_proof::LinearCombination;
    pub use circuit_proof::R1CSProof;
    pub use circuit_proof::Variable;
    pub use errors::R1CSError;
}
