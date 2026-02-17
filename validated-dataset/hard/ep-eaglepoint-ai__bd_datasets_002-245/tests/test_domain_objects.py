import unittest
from decimal import Decimal
try:
    from main import Money, Address, OrderStatus, InvalidOrderError
    HAS_DOMAIN_OBJECTS = True
except ImportError:
    HAS_DOMAIN_OBJECTS = False
    # Mock symbols so the class can be defined
    class Money: pass
    class Address: pass
    class OrderStatus: pass
    class InvalidOrderError(Exception): pass

class TestDomainObjects(unittest.TestCase):
    def setUp(self):
        if not HAS_DOMAIN_OBJECTS:
            self.fail("Refactored domain objects (Money, Address, OrderStatus) are missing in this implementation.")

    def test_money_arithmetic(self):
        m1 = Money.from_float(10.50)
        m2 = Money.from_float(5.25)
        
        # Add
        self.assertEqual((m1 + m2).to_float(), 15.75)
        # Subtract
        self.assertEqual((m1 - m2).to_float(), 5.25)
        # Multiply
        self.assertEqual((m1 * 2).to_float(), 21.00)
        self.assertEqual((m1 * Decimal('0.1')).to_float(), 1.05)
        
        # Compare
        self.assertTrue(m1 >= m2)
        self.assertFalse(m2 >= m1)
        self.assertTrue(m2 < m1)

    def test_money_rounding(self):
        # Test Banker's rounding (ROUND_HALF_EVEN)
        m_up = Money(Decimal('10.505'))
        self.assertEqual(m_up.to_float(), 10.50) # 0 is even
        m_up2 = Money(Decimal('10.515'))
        self.assertEqual(m_up2.to_float(), 10.52) # 2 is even

    def test_address_validation(self):
        # Valid
        addr = Address.from_dict({'country': 'US', 'state': 'NY'})
        self.assertEqual(addr.country, 'US')
        
        # Invalid types
        with self.assertRaises(InvalidOrderError):
            Address.from_dict({'country': 123})
        with self.assertRaises(InvalidOrderError):
            Address.from_dict("not a dict")

    def test_order_status_enum(self):
        self.assertEqual(OrderStatus.PENDING.value, 'pending')
        self.assertEqual(OrderStatus('pending'), OrderStatus.PENDING)
        with self.assertRaises(ValueError):
            OrderStatus('invalid_status')

if __name__ == '__main__':
    unittest.main()
