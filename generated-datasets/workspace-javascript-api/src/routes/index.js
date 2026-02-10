const express = require('express');
const authRoutes = require('./auth.routes');
const usersRoutes = require('./users.routes');

const router = express.Router();

router.use('/auth', authRoutes);
router.use('/users', usersRoutes);

router.get('/', (req, res) => {
  res.json({
    message: 'API Service v1.0',
    endpoints: {
      auth: '/api/auth',
      users: '/api/users',
    },
  });
});

module.exports = router;
