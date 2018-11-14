//! Definition of the constraint system trait.

use super::{Assignment, LinearCombination, R1CSError, Variable};
use curve25519_dalek::scalar::Scalar;

/// The interface for a constraint system, abstracting over the prover
/// and verifier's roles.
///
/// Statements to be proved by an [`R1CSProof`] are specified by
/// programmatically constructing constraints.  These constraints need
/// to be identical between the prover and verifier, since the prover
/// and verifier need to construct the same statement.
///
/// To prevent code duplication or mismatches between the prover and
/// verifier, gadgets for the constraint system should be written
/// using the `ConstraintSystem` trait, so that the prover and
/// verifier share the logic for specifying constraints.
pub trait ConstraintSystem {
    /// Allocate variables for left, right, and output wires of
    /// multiplication, and assign them the Assignments that are
    /// passed in.
    ///
    /// The `ProverCS` should pass `Value(Scalar)`s to synthesize the
    /// witness.
    ///
    /// The `VerifierCS` should pass `Missing` (since it does not have
    /// the witness).
    ///
    /// This allows the prover and verifier to use the same code for
    /// defining gadgets, eliminating the possibility of a constraint
    /// system mismatch.
    fn assign_multiplier(
        &mut self,
        left: Assignment,
        right: Assignment,
        out: Assignment,
    ) -> Result<(Variable, Variable, Variable), R1CSError>;

    /// Allocate two uncommitted variables, and assign them the
    /// `Assignments` passed in.
    ///
    /// The `ProverCS` should pass `Value(Scalar)`s to synthesize the
    /// witness.
    ///
    /// The `VerifierCS` should pass `Missing` (since it does not have
    /// the witness).
    ///
    /// This allows the prover and verifier to use the same code for
    /// defining gadgets, eliminating the possibility of a constraint
    /// system mismatch.
    fn assign_uncommitted(
        &mut self,
        val_1: Assignment,
        val_2: Assignment,
    ) -> Result<(Variable, Variable), R1CSError>;

    /// Enforce that the given `LinearCombination` is zero.
    fn add_constraint(&mut self, lc: LinearCombination);

    /// Obtain a challenge scalar bound to the assignments of all of
    /// the externally committed wires.
    ///
    /// This allows the prover to select a challenge circuit from a
    /// family of circuits parameterized by challenge scalars.
    ///
    /// # Warning
    ///
    /// The challenge scalars are bound only to the externally
    /// committed wires (high-level witness variables), and not to the
    /// assignments to all wires (low-level witness variables).  In
    /// the same way that it is the user's responsibility to ensure
    /// that the constraints are sound, it is **also** the user's
    /// responsibility to ensure that each challenge circuit is sound.
    fn challenge_scalar(&mut self, label: &'static [u8]) -> Scalar;
}
