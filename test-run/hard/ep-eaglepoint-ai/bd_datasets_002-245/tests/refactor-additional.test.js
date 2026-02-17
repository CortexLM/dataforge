/**
 * Additional tests for order processing refactor behavior
 */

const PaymentService = require('../repository_after/services/PaymentService');
const InventoryService = require('../repository_after/services/InventoryService');
const FraudCheckService = require('../repository_after/services/FraudCheckService');
const inventory = require('../repository_after/infra/inventory');
const { PaymentDeclinedError, FraudCheckFailedError } = require('../repository_after/errors/OrderProcessingError');

describe('Order processing refactor additional behavior', () => {
  beforeEach(() => {
    inventory.reset();
  });

  describe('PaymentService validation', () => {
    test('rejects missing payment token with standardized error', async () => {
      expect.assertions(2);
      try {
        await PaymentService.authorizePayment('', 42.75);
      } catch (error) {
        expect(error).toBeInstanceOf(PaymentDeclinedError);
        expect(error.details.errorType).toBe('INVALID_TOKEN');
      }
    });

    test('rejects non-positive amounts with standardized error', async () => {
      expect.assertions(2);
      try {
        await PaymentService.authorizePayment('valid-token', 0);
      } catch (error) {
        expect(error).toBeInstanceOf(PaymentDeclinedError);
        expect(error.details.errorType).toBe('INVALID_AMOUNT');
      }
    });
  });

  describe('InventoryService reservation rollback', () => {
    test('rolls back reservations if a later item cannot be reserved', async () => {
      inventory.initStock('item-ok', 2);
      inventory.initStock('item-low', 0);

      const result = await InventoryService.reserveStock([
        { id: 'item-ok', quantity: 1 },
        { id: 'item-low', quantity: 1 }
      ]);

      expect(result.success).toBe(false);
      expect(result.reservationsRolledBack).toBe(true);

      const remainingStock = await inventory.checkStock('item-ok');
      expect(remainingStock).toBe(2);
    });
  });

  describe('FraudCheckService risk evaluation', () => {
    test('flags fraudulent order when risk threshold exceeded', async () => {
      const orderData = {
        userId: '',
        items: Array.from({ length: 60 }, (_, i) => ({
          id: `item-${i}`,
          price: 300,
          quantity: 1
        })),
        paymentToken: ''
      };

      const result = await FraudCheckService.checkFraud(orderData);

      expect(result.passed).toBe(false);
      expect(result.error).toBeInstanceOf(FraudCheckFailedError);
      expect(result.riskScore).toBeGreaterThanOrEqual(80);
    });
  });
});
