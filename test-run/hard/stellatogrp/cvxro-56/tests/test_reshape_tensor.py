import importlib.util
import unittest
from pathlib import Path

import numpy as np
from scipy.sparse import coo_matrix, csc_matrix


def load_utils_module():
    utils_path = Path(__file__).resolve().parents[1] / "cvxro" / "uncertain_canon" / "utils.py"
    spec = importlib.util.spec_from_file_location("cvxro_uncertain_utils", utils_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class TestReshapeTensor(unittest.TestCase):
    def setUp(self):
        self.utils = load_utils_module()

    def test_reshape_tensor_permutation_and_type(self):
        n_var = 2
        num_constraints = 3
        n_var_full = n_var + 1
        num_rows = num_constraints * n_var_full
        # Build a dense matrix with distinct row patterns
        dense = np.arange(num_rows * 4, dtype=float).reshape(num_rows, 4)
        T_Ab = coo_matrix(dense)

        reshaped = self.utils.reshape_tensor(T_Ab, n_var)

        target_rows = np.arange(num_rows)
        constraint_nums = target_rows % n_var_full
        var_nums = target_rows // n_var_full
        perm = constraint_nums * num_constraints + var_nums
        expected = csc_matrix(dense)[perm, :]

        self.assertIsInstance(reshaped, csc_matrix)
        self.assertEqual(reshaped.shape, expected.shape)
        self.assertTrue(np.allclose(reshaped.toarray(), expected.toarray()))

    def test_reshape_tensor_zero_variables_identity(self):
        n_var = 0
        num_constraints = 4
        num_rows = num_constraints * (n_var + 1)
        dense = np.array([
            [1.0, 0.0, -2.0],
            [0.0, 3.5, 4.0],
            [5.0, -6.0, 0.0],
            [7.0, 8.0, 9.0],
        ])
        self.assertEqual(dense.shape[0], num_rows)
        T_Ab = coo_matrix(dense)

        reshaped = self.utils.reshape_tensor(T_Ab, n_var)

        self.assertIsInstance(reshaped, csc_matrix)
        self.assertEqual(reshaped.nnz, csc_matrix(dense).nnz)
        self.assertTrue(np.allclose(reshaped.toarray(), dense))


if __name__ == "__main__":
    unittest.main()
