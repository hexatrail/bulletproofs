#![allow(non_snake_case)]
#![allow(non_camel_case_types)]

use rand::{CryptoRng, Rng};

use super::circuit::{Circuit, ProverInput, VerifierInput};
use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;
use errors::R1CSError;
use generators::PedersenGenerators;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum VariableType {
    v,  // high-level variable
    aL, // low-level variable, left input of multiplication gate
    aR, // low-level variable, right input of multiplication gate
    aO, // low-level variable, output multiplication gate
}

/// Represents a V variable in our constraint system, where the value represents the index.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Variable {
    var_type: VariableType,
    index: usize,
}

pub type Assignment = Result<Scalar, R1CSError>;

pub fn err_assignment() -> Assignment {
    Err(R1CSError::InvalidVariableAssignment)
}

/// Represents a linear combination of some variables multiplied with their scalar coefficients,
/// plus a scalar. The linear combination is supposed to evaluate to zero.
/// E.g. LC = 0 = variable[0]*scalar[0] + variable[1]*scalar[1] + scalar
#[derive(Clone, Debug)]
pub struct LinearCombination {
    variables: Vec<(Variable, Scalar)>,
    constant: Scalar,
}

impl LinearCombination {
    // TODO: make constructor with iterators
    // see FromIterator trait - [(a1, v1), (a2, v2)].iter().collect() (pass in the iterator, collect to get LC)
    pub fn new(variables: Vec<(Variable, Scalar)>, constant: Scalar) -> Self {
        LinearCombination {
            variables,
            constant,
        }
    }

    pub fn zero() -> Self {
        LinearCombination {
            variables: vec![],
            constant: Scalar::zero(),
        }
    }

    pub fn get_variables(&self) -> Vec<(Variable, Scalar)> {
        self.variables.clone()
    }

    pub fn get_constant(&self) -> Scalar {
        self.constant.clone()
    }
}

pub struct ConstraintSystem {
    lc_vec: Vec<LinearCombination>,

    // variable assignments
    aL_assignment: Vec<Assignment>,
    aR_assignment: Vec<Assignment>,
    aO_assignment: Vec<Assignment>,
    v_assignment: Vec<Assignment>,
}

impl ConstraintSystem {
    pub fn new() -> Self {
        ConstraintSystem {
            lc_vec: vec![],
            aL_assignment: vec![],
            aR_assignment: vec![],
            aO_assignment: vec![],
            v_assignment: vec![],
        }
    }

    // Allocate variables for aL, aR, aO, and assign them the Result values that are passed in.
    // Prover will pass in Ok(Scalar)s, and Verifier will pass in R1CSErrors.
    pub fn assign_a(
        &mut self,
        aL_val: Assignment,
        aR_val: Assignment,
        aO_val: Assignment,
    ) -> (Variable, Variable, Variable) {
        let aL_var = self.make_variable(VariableType::aL, aL_val);
        let aR_var = self.make_variable(VariableType::aR, aR_val);
        let aO_var = self.make_variable(VariableType::aO, aO_val);
        (aL_var, aR_var, aO_var)
    }

    // Allocate a variable for v, and assign it the Result value passed in.
    // Prover will pass in Ok(Scalar), and Verifier will pass in R1CSError.
    pub fn assign_v(&mut self, v_val: Assignment) -> Variable {
        self.make_variable(VariableType::v, v_val)
    }

    fn make_variable(&mut self, var_type: VariableType, value: Assignment) -> Variable {
        let index = match var_type {
            VariableType::aL => {
                self.aL_assignment.push(value);
                self.aL_assignment.len() - 1
            }
            VariableType::aR => {
                self.aR_assignment.push(value);
                self.aR_assignment.len() - 1
            }
            VariableType::aO => {
                self.aO_assignment.push(value);
                self.aO_assignment.len() - 1
            }
            VariableType::v => {
                self.v_assignment.push(value);
                self.v_assignment.len() - 1
            }
        };
        Variable { var_type, index }
    }

    // Get number of multiplications.
    pub fn get_n(&self) -> usize {
        let n = self.aL_assignment.len();
        if n == 0 || n.is_power_of_two() {
            return n;
        }
        return n.next_power_of_two();
    }

    // Get number of high-level witness variables.
    pub fn get_m(&self) -> usize {
        self.v_assignment.len()
    }

    pub fn make_V(
        &self,
        pedersen_gens: &PedersenGenerators,
        v_blinding: &Vec<Scalar>,
    ) -> Result<Vec<RistrettoPoint>, R1CSError> {
        if v_blinding.len() != self.get_m() {
            return Err(R1CSError::IncorrectInputSize);
        }
        self.v_assignment
            .iter()
            .zip(v_blinding)
            .map(|(v_i, v_blinding_i)| Ok(pedersen_gens.commit(v_i.clone()?, *v_blinding_i)))
            .collect()
    }

    pub fn constrain(&mut self, lc: LinearCombination) {
        // TODO: check that the linear combinations are valid
        // (e.g. that variables are valid, that the linear combination evals to 0 for prover, etc).
        self.lc_vec.push(lc);
    }

    fn create_prover_input(&self, v_blinding: &Vec<Scalar>) -> Result<ProverInput, R1CSError> {
        let aL = self
            .aL_assignment
            .iter()
            .cloned()
            .collect::<Result<Vec<_>, _>>()?;
        let aR = self
            .aR_assignment
            .iter()
            .cloned()
            .collect::<Result<Vec<_>, _>>()?;
        let aO = self
            .aO_assignment
            .iter()
            .cloned()
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ProverInput::new(aL, aR, aO, v_blinding.to_vec()))
    }

    fn create_verifier_input(
        &self,
        pedersen_gens: &PedersenGenerators,
        v_blinding: &Vec<Scalar>,
    ) -> Result<VerifierInput, R1CSError> {
        Ok(VerifierInput::new(self.make_V(pedersen_gens, v_blinding)?))
    }

    fn create_circuit(&self) -> Circuit {
        let n = self.get_n();
        let m = self.v_assignment.len();
        let q = self.lc_vec.len();

        let zer = Scalar::zero();
        let mut W_L = vec![vec![zer; n]; q]; // qxn matrix which corresponds to a.
        let mut W_R = vec![vec![zer; n]; q]; // qxn matrix which corresponds to b.
        let mut W_O = vec![vec![zer; n]; q]; // qxn matrix which corresponds to c.
        let mut W_V = vec![vec![zer; m]; q]; // qxm matrix which corresponds to v
        let mut c = vec![zer; q]; // length q vector of constants.

        for (index, lc) in self.lc_vec.iter().enumerate() {
            for (var, coeff) in lc.variables.clone() {
                match var.var_type {
                    VariableType::aL => W_L[index][var.index] = -coeff,
                    VariableType::aR => W_R[index][var.index] = -coeff,
                    VariableType::aO => W_O[index][var.index] = -coeff,
                    VariableType::v => W_V[index][var.index] = coeff,
                };
            }
            c[index] = lc.constant
        }

        Circuit::new(n, m, q, c, W_L, W_R, W_O, W_V)
    }

    // This function can only be called once per ConstraintSystem instance.
    pub fn create_proof_input<R: Rng + CryptoRng>(
        mut self,
        pedersen_gens: &PedersenGenerators,
        rng: &mut R,
    ) -> (
        Circuit,
        Result<ProverInput, R1CSError>,
        Result<VerifierInput, R1CSError>,
    ) {
        // If `n`, the number of multiplications, is not 0 or 2, then pad the circuit.
        let n = self.aL_assignment.len();
        if !(n == 0 || n.is_power_of_two()) {
            let pad = n.next_power_of_two() - n;
            let zer = Scalar::zero();
            for _ in 0..pad {
                self.assign_a(Ok(zer), Ok(zer), Ok(zer));
            }
        }
        let v_blinding: Vec<Scalar> = (0..self.get_m()).map(|_| Scalar::random(rng)).collect();

        let circuit = self.create_circuit();
        let prover_input = self.create_prover_input(&v_blinding);
        let verifier_input = self.create_verifier_input(pedersen_gens, &v_blinding);

        (circuit, prover_input, verifier_input)
    }
}

#[cfg(test)]
mod tests {
    use super::super::circuit::CircuitProof;
    use super::*;
    use generators::Generators;
    use proof_transcript::ProofTranscript;
    use rand::rngs::OsRng;

    fn create_and_verify_helper(
        prover_cs: ConstraintSystem,
        verifier_cs: ConstraintSystem,
        expected_result: Result<(), ()>,
    ) -> Result<(), R1CSError> {
        let mut rng = OsRng::new().unwrap();
        let pedersen_gens = PedersenGenerators::default();
        let generators = Generators::new(pedersen_gens, prover_cs.get_n(), 1);

        let (prover_circuit, prover_input, verifier_input) =
            prover_cs.create_proof_input(&pedersen_gens, &mut rng);
        assert!(prover_input.is_ok());
        assert!(verifier_input.is_ok());

        let mut prover_transcript = ProofTranscript::new(b"CircuitProofTest");
        let circuit_proof = CircuitProof::prove(
            &generators,
            &mut prover_transcript,
            &mut rng,
            &prover_circuit,
            &prover_input?,
        );

        let (verifier_circuit, _, _) = verifier_cs.create_proof_input(&pedersen_gens, &mut rng);

        assert_eq!(prover_circuit, verifier_circuit);

        let mut verifier_transcript = ProofTranscript::new(b"CircuitProofTest");
        let actual_result = circuit_proof.unwrap().verify(
            &generators,
            &mut verifier_transcript,
            &mut rng,
            &verifier_circuit,
            &verifier_input?,
        );

        println!("expected result: {:?}", expected_result);
        println!("actual result: {:?}", actual_result);
        match expected_result {
            Ok(_) => assert!(actual_result.is_ok()),
            Err(_) => assert!(actual_result.is_err()),
        }

        Ok(())
    }

    // a * b =? c
    // The purpose of this test is to see how a multiplication gate with no
    // variables (no corresponding v commitments) and no linear constraints behaves.
    fn mul_circuit_basic_helper(a: u64, b: u64, c: u64, expected_result: Result<(), ()>) {
        let mut prover_cs = ConstraintSystem::new();
        prover_cs.assign_a(
            Ok(Scalar::from(a)),
            Ok(Scalar::from(b)),
            Ok(Scalar::from(c)),
        );

        let mut verifier_cs = ConstraintSystem::new();
        verifier_cs.assign_a(err_assignment(), err_assignment(), err_assignment());

        assert!(create_and_verify_helper(prover_cs, verifier_cs, expected_result).is_ok());
    }

    #[test]
    fn mul_circuit_basic() {
        mul_circuit_basic_helper(3, 4, 12, Ok(())); // 3 * 4 = 12
        mul_circuit_basic_helper(3, 4, 10, Err(())); // 3 * 4 != 10
    }

    // (a * a_coeff) * (b * b_coeff) =? c * c_coeff
    // Where we define a, b, c as low-level variables (aL and aR variables) then then
    // tie those to high-level variables (v variables). The purpose of this test is to
    // see if we can successfully tie the low-level and high-level variables together.
    fn mul_circuit_helper(
        a: u64,
        a_coeff: u64,
        b: u64,
        b_coeff: u64,
        c: u64,
        c_coeff: u64,
        expected_result: Result<(), ()>,
    ) {
        let one = Scalar::one();
        let zer = Scalar::zero();

        let mut prover_cs = ConstraintSystem::new();
        let (aL, aR, aO) = prover_cs.assign_a(
            Ok(Scalar::from(a) * Scalar::from(a_coeff)),
            Ok(Scalar::from(b) * Scalar::from(b_coeff)),
            Ok(Scalar::from(c) * Scalar::from(c_coeff)),
        );
        let v_a = prover_cs.assign_v(Ok(Scalar::from(a)));
        let v_b = prover_cs.assign_v(Ok(Scalar::from(b)));
        let v_c = prover_cs.assign_v(Ok(Scalar::from(c)));

        prover_cs.constrain(LinearCombination::new(
            vec![(aL, -one), (v_a, Scalar::from(a_coeff))],
            zer,
        ));
        prover_cs.constrain(LinearCombination::new(
            vec![(aR, -one), (v_b, Scalar::from(b_coeff))],
            zer,
        ));
        prover_cs.constrain(LinearCombination::new(
            vec![(aO, -one), (v_c, Scalar::from(c_coeff))],
            zer,
        ));

        let mut verifier_cs = ConstraintSystem::new();
        let (aL, aR, aO) =
            verifier_cs.assign_a(err_assignment(), err_assignment(), err_assignment());
        let v_a = verifier_cs.assign_v(err_assignment());
        let v_b = verifier_cs.assign_v(err_assignment());
        let v_c = verifier_cs.assign_v(err_assignment());

        verifier_cs.constrain(LinearCombination::new(
            vec![(aL, -one), (v_a, Scalar::from(a_coeff))],
            zer,
        ));
        verifier_cs.constrain(LinearCombination::new(
            vec![(aR, -one), (v_b, Scalar::from(b_coeff))],
            zer,
        ));
        verifier_cs.constrain(LinearCombination::new(
            vec![(aO, -one), (v_c, Scalar::from(c_coeff))],
            zer,
        ));

        assert!(create_and_verify_helper(prover_cs, verifier_cs, expected_result).is_ok());
    }

    #[test]
    fn mul_circuit() {
        // test multiplication without coefficients
        mul_circuit_helper(3, 1, 4, 1, 12, 1, Ok(())); // (3*1) * (4*1) = (12*1)
        mul_circuit_helper(3, 1, 4, 1, 10, 1, Err(())); // (3*1) * (4*1) != (10*1)

        // test multiplication with coefficients
        mul_circuit_helper(3, 2, 4, 5, 120, 1, Ok(())); // (3*2) * (4*5) = (120*1)
        mul_circuit_helper(3, 2, 4, 5, 121, 1, Err(())); // (3*2) * (4*5) != (121*1)

        // test multiplication with zeros
        mul_circuit_helper(0, 2, 4, 5, 120, 0, Ok(())); // (0*2) * (4*5) = (120*0)
        mul_circuit_helper(0, 2, 4, 5, 120, 1, Err(())); // (0*2) * (4*5) = (120*1)
    }

    // a + b =? c
    // The purpose of this test is to see how a circuit with no multiplication gates,
    // and one addition gate, behaves.
    fn add_circuit_basic_helper(a: u64, b: u64, c: u64, expected_result: Result<(), ()>) {
        let one = Scalar::one();
        let zer = Scalar::zero();

        let mut prover_cs = ConstraintSystem::new();
        let v_a = prover_cs.assign_v(Ok(Scalar::from(a)));
        let v_b = prover_cs.assign_v(Ok(Scalar::from(b)));
        let v_c = prover_cs.assign_v(Ok(Scalar::from(c)));
        prover_cs.constrain(LinearCombination::new(
            vec![(v_a, one), (v_b, one), (v_c, -one)],
            zer,
        ));

        let mut verifier_cs = ConstraintSystem::new();
        let v_a = verifier_cs.assign_v(err_assignment());
        let v_b = verifier_cs.assign_v(err_assignment());
        let v_c = verifier_cs.assign_v(err_assignment());
        verifier_cs.constrain(LinearCombination::new(
            vec![(v_a, one), (v_b, one), (v_c, -one)],
            zer,
        ));

        assert!(create_and_verify_helper(prover_cs, verifier_cs, expected_result).is_ok());
    }

    #[test]
    fn add_circuit_basic() {
        add_circuit_basic_helper(3, 4, 7, Ok(())); // 3 + 4 = 7
        add_circuit_basic_helper(3, 4, 10, Err(())); // 3 + 4 != 10
    }

    // a + b =? c
    // Where we define a, b, c as low-level variables (aL and aR variables) then then
    // tie those to high-level variables (v variables). The purpose of this test is to
    // check that low-level variable allocation works, and see that we can successfully
    // tie the low-level and high-level variables together.
    fn add_circuit_helper(a: u64, b: u64, c: u64, expected_result: Result<(), ()>) {
        let one = Scalar::one();
        let zer = Scalar::zero();

        let mut prover_cs = ConstraintSystem::new();
        // Make high-level variables
        let v_a = prover_cs.assign_v(Ok(Scalar::from(a)));
        let v_b = prover_cs.assign_v(Ok(Scalar::from(b)));
        let v_c = prover_cs.assign_v(Ok(Scalar::from(c)));
        // Make low-level variables (aL_0 = v_a, aR_0 = v_b, aL_1 = v_c)
        let (aL_0, aR_0, _) = prover_cs.assign_a(
            Ok(Scalar::from(a)),
            Ok(Scalar::from(b)),
            Ok(Scalar::from(a * b)),
        );
        let (aL_1, _, _) = prover_cs.assign_a(Ok(Scalar::from(c)), Ok(zer), Ok(zer));
        // Tie high-level and low-level variables together
        prover_cs.constrain(LinearCombination::new(vec![(aL_0, -one), (v_a, one)], zer));
        prover_cs.constrain(LinearCombination::new(vec![(aR_0, -one), (v_b, one)], zer));
        prover_cs.constrain(LinearCombination::new(vec![(aL_1, -one), (v_c, one)], zer));
        // Addition logic (using low-level variables)
        prover_cs.constrain(LinearCombination::new(
            vec![(aL_0, one), (aR_0, one), (aL_1, -one)],
            zer,
        ));

        let mut verifier_cs = ConstraintSystem::new();
        // Make high-level variables
        let v_a = verifier_cs.assign_v(err_assignment());
        let v_b = verifier_cs.assign_v(err_assignment());
        let v_c = verifier_cs.assign_v(err_assignment());
        // Make low-level variables (aL_0 = v_a, aR_0 = v_b, aL_1 = v_c)
        let (aL_0, aR_0, _) =
            verifier_cs.assign_a(err_assignment(), err_assignment(), err_assignment());
        let (aL_1, _, _) =
            verifier_cs.assign_a(err_assignment(), err_assignment(), err_assignment());
        // Tie high-level and low-level variables together
        verifier_cs.constrain(LinearCombination::new(vec![(aL_0, -one), (v_a, one)], zer));
        verifier_cs.constrain(LinearCombination::new(vec![(aR_0, -one), (v_b, one)], zer));
        verifier_cs.constrain(LinearCombination::new(vec![(aL_1, -one), (v_c, one)], zer));
        // Addition logic (using low-level variables)
        verifier_cs.constrain(LinearCombination::new(
            vec![(aL_0, one), (aR_0, one), (aL_1, -one)],
            zer,
        ));

        assert!(create_and_verify_helper(prover_cs, verifier_cs, expected_result).is_ok());
    }

    #[test]
    fn add_circuit() {
        add_circuit_helper(3, 4, 7, Ok(())); // 3 + 4 = 7
        add_circuit_helper(3, 4, 10, Err(())); // 3 + 4 != 10
    }

    // (a1 + a2) * (b1 + b2) =? (c1 + c2)
    // Where a1, a2, b1, b2, c1, c2 are all allocated as high-level variables
    fn mixed_circuit_helper(
        a1: u64,
        a2: u64,
        b1: u64,
        b2: u64,
        c1: u64,
        c2: u64,
        expected_result: Result<(), ()>,
    ) {
        let one = Scalar::one();
        let zer = Scalar::zero();

        let mut prover_cs = ConstraintSystem::new();
        // Make high-level variables
        let v_a1 = prover_cs.assign_v(Ok(Scalar::from(a1)));
        let v_a2 = prover_cs.assign_v(Ok(Scalar::from(a2)));
        let v_b1 = prover_cs.assign_v(Ok(Scalar::from(b1)));
        let v_b2 = prover_cs.assign_v(Ok(Scalar::from(b2)));
        let v_c1 = prover_cs.assign_v(Ok(Scalar::from(c1)));
        let v_c2 = prover_cs.assign_v(Ok(Scalar::from(c2)));
        // Make low-level variables (aL = v_a1 + v_a2, aR = v_b1 + v_b2, aO = v_c1 + v_c2)
        let (aL, aR, aO) = prover_cs.assign_a(
            Ok(Scalar::from(a1 + a2)),
            Ok(Scalar::from(b1 + b2)),
            Ok(Scalar::from(c1 + c2)),
        );
        // Tie high-level and low-level variables together
        prover_cs.constrain(LinearCombination::new(
            vec![(aL, -one), (v_a1, one), (v_a2, one)],
            zer,
        ));
        prover_cs.constrain(LinearCombination::new(
            vec![(aR, -one), (v_b1, one), (v_b2, one)],
            zer,
        ));
        prover_cs.constrain(LinearCombination::new(
            vec![(aO, -one), (v_c1, one), (v_c2, one)],
            zer,
        ));

        let mut verifier_cs = ConstraintSystem::new();
        // Make high-level variables
        let v_a1 = verifier_cs.assign_v(Ok(Scalar::from(a1)));
        let v_a2 = verifier_cs.assign_v(Ok(Scalar::from(a2)));
        let v_b1 = verifier_cs.assign_v(Ok(Scalar::from(b1)));
        let v_b2 = verifier_cs.assign_v(Ok(Scalar::from(b2)));
        let v_c1 = verifier_cs.assign_v(Ok(Scalar::from(c1)));
        let v_c2 = verifier_cs.assign_v(Ok(Scalar::from(c2)));
        // Make low-level variables (aL = v_a1 + v_a2, aR = v_b1 + v_b2, aO = v_c1 + v_c2)
        let (aL, aR, aO) = verifier_cs.assign_a(
            Ok(Scalar::from(a1 + a2)),
            Ok(Scalar::from(b1 + b2)),
            Ok(Scalar::from(c1 + c2)),
        );
        // Tie high-level and low-level variables together
        verifier_cs.constrain(LinearCombination::new(
            vec![(aL, -one), (v_a1, one), (v_a2, one)],
            zer,
        ));
        verifier_cs.constrain(LinearCombination::new(
            vec![(aR, -one), (v_b1, one), (v_b2, one)],
            zer,
        ));
        verifier_cs.constrain(LinearCombination::new(
            vec![(aO, -one), (v_c1, one), (v_c2, one)],
            zer,
        ));

        assert!(create_and_verify_helper(prover_cs, verifier_cs, expected_result).is_ok());
    }

    #[test]
    fn mixed_circuit() {
        mixed_circuit_helper(3, 4, 6, 1, 40, 9, Ok(())); // (3 + 4) * (6 + 1) = (40 + 9)
        mixed_circuit_helper(3, 4, 6, 1, 40, 10, Err(())); // (3 + 4) * (6 + 1) != (40 + 10)
    }
}
