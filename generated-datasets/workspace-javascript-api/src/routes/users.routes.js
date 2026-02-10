const express = require('express');
const usersController = require('../controllers/users.controller');
const { authenticate, requireRole } = require('../middleware/auth');
const { validateUserUpdate } = require('../middleware/validation');

const router = express.Router();

router.get('/', authenticate, usersController.getAllUsers);

router.get('/search', authenticate, usersController.searchUsers);

router.get('/profile/:username', usersController.getPublicProfile);

router.get('/:id', authenticate, usersController.getUserById);

router.put('/:id', authenticate, validateUserUpdate, usersController.updateUser);

router.delete('/:id', authenticate, requireRole('admin'), usersController.deleteUser);

router.post('/:id/settings', authenticate, usersController.updateSettings);

router.get('/:id/export', authenticate, usersController.exportUserData);

module.exports = router;
