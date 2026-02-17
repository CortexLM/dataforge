import unittest
try:
    from main import (
        OrderProcessor, InvalidOrderError, ProductNotFoundError, 
        InsufficientInventoryError, OrderStatus
    )
    HAS_REFACTORED_FEATURES = True
except ImportError:
    # Fallback for repository_before
    from main import OrderProcessor
    HAS_REFACTORED_FEATURES = False
    InvalidOrderError = Exception
    ProductNotFoundError = Exception
    InsufficientInventoryError = Exception

class TestOrderRefactoring(unittest.TestCase):
    def setUp(self):
        self.processor = OrderProcessor()
        self.processor.add_product("p1", "Product 1", 100.0, 10, 1.0)

    def test_invalid_status(self):
        order_data = {
            'customer': {'id': 'c1'},
            'items': [{'product_id': 'p1', 'quantity': 1}],
            'shipping_address': {'country': 'US', 'state': 'NY'}
        }
        order = self.processor.process_order(order_data)
        order_id = order['order_id']
        
        with self.assertRaises(InvalidOrderError) as cm:
            self.processor.update_order_status(order_id, "SUPER_PAID")
        self.assertIn("Invalid status: SUPER_PAID", str(cm.exception))
        # Extra check: ensure it's NOT just a generic Exception if we are expecting refactored features
        if type(cm.exception) is Exception and InvalidOrderError is not Exception:
            self.fail("Raised generic Exception instead of InvalidOrderError")

    def test_negative_quantity(self):
        order_data = {
            'customer': {'id': 'c1'},
            'items': [{'product_id': 'p1', 'quantity': -1}],
            'shipping_address': {'country': 'US', 'state': 'NY'}
        }
        with self.assertRaises(InvalidOrderError) as cm:
            self.processor.process_order(order_data)
        self.assertIn("Invalid quantity -1", str(cm.exception))

    def test_missing_data(self):
        with self.assertRaises(Exception):
            self.processor.process_order({})
        with self.assertRaises(Exception):
            self.processor.process_order({'customer': {'id': 'c1'}}) # missing items

    def test_insufficient_stock_context(self):
        order_data = {
            'customer': {'id': 'c1'},
            'items': [{'product_id': 'p1', 'quantity': 100}],
            'shipping_address': {'country': 'US', 'state': 'NY'}
        }
        with self.assertRaises(InsufficientInventoryError) as cm:
            self.processor.process_order(order_data)
        
        self.assertEqual(cm.exception.product_id, "p1")
        self.assertEqual(cm.exception.requested, 100)
        self.assertEqual(cm.exception.available, 10)
        self.assertEqual(cm.exception.context['requested'], 100) 
        self.assertIn("Insufficient stock for product p1", str(cm.exception))

    def test_address_validation_missing_fields(self):
        # Requirement 10: missing fields must raise exceptions
        base_order = {
            'customer': {'id': 'c1'},
            'items': [{'product_id': 'p1', 'quantity': 1}]
        }
        
        # Missing shipping_address
        with self.assertRaises(InvalidOrderError) as cm:
            self.processor.process_order(base_order)
        self.assertIn("Order must have shipping address", str(cm.exception))
        
        # Missing country
        with self.assertRaises(InvalidOrderError) as cm:
            self.processor.process_order({**base_order, 'shipping_address': {'state': 'NY'}})
        self.assertIn("Missing required field: country", str(cm.exception))
        
        # Missing state
        with self.assertRaises(InvalidOrderError) as cm:
            self.processor.process_order({**base_order, 'shipping_address': {'country': 'US'}})
        self.assertIn("Missing required field: state", str(cm.exception))

    def test_status_transitions(self):
        # Requirement 10: cover all valid/invalid status transitions
        order_data = {
            'customer': {'id': 'c1'},
            'items': [{'product_id': 'p1', 'quantity': 1}],
            'shipping_address': {'country': 'US', 'state': 'NY'}
        }
        order = self.processor.process_order(order_data)
        order_id = order['order_id']
        
        # PENDING -> PAID (Valid)
        self.processor.update_order_status(order_id, 'paid')
        self.assertEqual(self.processor.get_order(order_id)['status'], 'paid')
        
        # PAID -> SHIPPED (Valid)
        self.processor.update_order_status(order_id, 'shipped')
        self.assertEqual(self.processor.get_order(order_id)['status'], 'shipped')
        
        # SHIPPED -> DELIVERED (Valid)
        self.processor.update_order_status(order_id, 'delivered')
        self.assertEqual(self.processor.get_order(order_id)['status'], 'delivered')
        
        # DELIVERED -> CANCELLED (Invalid)
        with self.assertRaises(InvalidOrderError) as cm:
            self.processor.update_order_status(order_id, 'cancelled')
        self.assertIn("Invalid transition from delivered to cancelled", str(cm.exception))
        
        # SHIPPED -> PAID (Invalid - reset to shipped for test)
        order2 = self.processor.process_order(order_data)
        oid2 = order2['order_id']
        self.processor.update_order_status(oid2, 'paid')
        self.processor.update_order_status(oid2, 'shipped')
        with self.assertRaises(InvalidOrderError) as cm:
            self.processor.update_order_status(oid2, 'paid')
        self.assertIn("Invalid transition from shipped to paid", str(cm.exception))

if __name__ == '__main__':
    unittest.main()
