function validateEmail(email) {
  const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
  return emailRegex.test(email);
}

function validatePassword(password) {
  return password && password.length >= 6;
}

function validateRegistration(req, res, next) {
  const { email, password, name } = req.body;
  const errors = [];

  if (!email || !validateEmail(email)) {
    errors.push('Valid email is required');
  }

  if (!validatePassword(password)) {
    errors.push('Password must be at least 6 characters');
  }

  if (!name || name.trim().length < 2) {
    errors.push('Name must be at least 2 characters');
  }

  if (errors.length > 0) {
    return res.status(400).json({ errors });
  }

  next();
}

function validateLogin(req, res, next) {
  const { email, password } = req.body;
  const errors = [];

  if (!email || !validateEmail(email)) {
    errors.push('Valid email is required');
  }

  if (!password) {
    errors.push('Password is required');
  }

  if (errors.length > 0) {
    return res.status(400).json({ errors });
  }

  next();
}

function validateUserUpdate(req, res, next) {
  const { email, name } = req.body;
  const errors = [];

  if (email && !validateEmail(email)) {
    errors.push('Invalid email format');
  }

  if (name && name.trim().length < 2) {
    errors.push('Name must be at least 2 characters');
  }

  if (errors.length > 0) {
    return res.status(400).json({ errors });
  }

  next();
}

function sanitizeInput(input) {
  if (typeof input !== 'string') return input;
  return input.trim();
}

module.exports = {
  validateEmail,
  validatePassword,
  validateRegistration,
  validateLogin,
  validateUserUpdate,
  sanitizeInput,
};
