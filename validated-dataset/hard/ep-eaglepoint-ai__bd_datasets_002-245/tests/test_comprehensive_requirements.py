import unittest
from datetime import datetime
from unittest.mock import patch
import concurrent.futures
from main import (
    OrderProcessor, PaymentFailedError, ShippingNotAvailableError,
    InsufficientInventoryError, InvalidOrderError, OrderStatus
)

class TestComprehensiveRequirements(unittest.TestCase):
    def setUp(self):
        self.processor = OrderProcessor()
        self.processor.add_product("p1", "Product 1", 100.0, 50, 1.0)
        self.processor.add_product("p2", "Product 2", 50.0, 100, 0.5)

    def test_req4_payment_failed_error(self):
        order_data = {
            'customer': {'id': 'c1', 'tier': 'standard'},
            'items': [{'product_id': 'p1', 'quantity': 1}],
            'shipping_address': {'country': 'US', 'state': 'NY'},
            'simulate_payment_failure': True
        }
        with self.assertRaises(PaymentFailedError) as cm:
            self.processor.process_order(order_data)
        self.assertIn("Payment failed for amount", str(cm.exception))

    def test_req4_shipping_not_available_error(self):
        order_data = {
            'customer': {'id': 'c1', 'tier': 'standard'},
            'items': [{'product_id': 'p1', 'quantity': 1}],
            'shipping_address': {'country': ' Mars ', 'state': ' Olympus Mons '}
        }
        with self.assertRaises(ShippingNotAvailableError) as cm:
            self.processor.process_order(order_data)
        self.assertIn("No shipping routes for  Mars ", str(cm.exception))

    def test_req7_shipping_boundaries(self):
        thresholds = [
            (1.0, 5.99),
            (1.1, 9.99),
            (5.0, 9.99),
            (5.1, 14.99),
            (20.0, 14.99),
            (20.1, 24.99)
        ]
        for weight, expected_cost in thresholds:
            p = OrderProcessor()
            p.add_product("px", "PX", 10.0, 10, weight)
            order_data = {
                'customer': {'id': 'c1'},
                'items': [{'product_id': 'px', 'quantity': 1}],
                'shipping_address': {'country': 'US', 'state': 'OR'}
            }
            order = p.process_order(order_data)
            self.assertEqual(order['shipping_cost'], expected_cost, f"Failed for weight {weight}")

    def test_req12_serial_and_inventory_deduction(self):
        order_data = {
            'customer': {'id': 'c1', 'tier': 'standard'},
            'items': [{'product_id': 'p1', 'quantity': 10}],
            'shipping_address': {'country': 'US', 'state': 'NY'}
        }
        self.processor.process_order(order_data)
        self.assertEqual(self.processor.inventory['p1']['stock'], 40)
        self.processor.process_order(order_data)
        self.assertEqual(self.processor.inventory['p1']['stock'], 30)

    def test_req12_atomicity_on_failure(self):
        initial_stock = self.processor.inventory['p1']['stock']
        order_data = {
            'customer': {'id': 'c1', 'tier': 'standard'},
            'items': [{'product_id': 'p1', 'quantity': 10}],
            'shipping_address': {'country': 'US', 'state': 'NY'},
            'simulate_payment_failure': True
        }
        with self.assertRaises(PaymentFailedError):
            self.processor.process_order(order_data)
        self.assertEqual(self.processor.inventory['p1']['stock'], initial_stock)

    def test_req13_state_isolation(self):
        p1 = OrderProcessor()
        p2 = OrderProcessor()
        p1.add_product("item", "name", 100.0, 10, 1.0)
        p2.add_product("item", "name", 100.0, 20, 1.0)
        self.assertEqual(p1.inventory["item"]["stock"], 10)
        self.assertEqual(p2.inventory["item"]["stock"], 20)
        p1.inventory["item"]["stock"] = 5
        self.assertEqual(p2.inventory["item"]["stock"], 20)

    def test_req17_timestamp_predictability(self):
        mock_time = datetime(2025, 1, 1, 12, 0, 0)
        with patch('main.datetime') as mock_datetime:
            mock_datetime.now.return_value = mock_time
            mock_datetime.strftime = datetime.strftime
            order_data = {
                'customer': {'id': 'c1'},
                'items': [{'product_id': 'p1', 'quantity': 1}],
                'shipping_address': {'country': 'US', 'state': 'NY'}
            }
            order = self.processor.process_order(order_data)
            created_at = order['created_at']
            try:
                datetime.fromisoformat(created_at)
            except ValueError:
                self.fail(f"created_at {created_at} is not in ISO format")

    def test_req13_concurrent_isolation_simulation(self):
        def run_independent_order(processor_idx):
            proc = OrderProcessor()
            proc.add_product(f"item_{processor_idx}", "Name", 100.0, 10, 1.0)
            order_data = {
                'customer': {'id': f'cust_{processor_idx}'},
                'items': [{'product_id': f'item_{processor_idx}', 'quantity': 1}],
                'shipping_address': {'country': 'US', 'state': 'NY'}
            }
            order = proc.process_order(order_data)
            return proc.inventory[f'item_{processor_idx}']['stock']

        with concurrent.futures.ThreadPoolExecutor(max_workers=5) as executor:
            results = list(executor.map(run_independent_order, range(10)))
        for stock in results:
            self.assertEqual(stock, 9, "Threaded execution resulted in inconsistent state")

if __name__ == '__main__':
    unittest.main()
