const mongoose = require('mongoose');

const userSchema = new mongoose.Schema({
  email: {
    type: String,
    required: true,
    unique: true,
    lowercase: true,
    trim: true,
  },
  password: {
    type: String,
    required: true,
  },
  name: {
    type: String,
    required: true,
    trim: true,
  },
  bio: {
    type: String,
    default: '',
    maxlength: 500,
  },
  avatar: {
    type: String,
    default: '',
  },
  role: {
    type: String,
    enum: ['user', 'moderator', 'admin'],
    default: 'user',
  },
  isActive: {
    type: Boolean,
    default: true,
  },
  settings: {
    type: mongoose.Schema.Types.Mixed,
    default: {},
  },
  preferences: {
    theme: {
      type: String,
      enum: ['light', 'dark', 'system'],
      default: 'system',
    },
    notifications: {
      email: { type: Boolean, default: true },
      push: { type: Boolean, default: false },
    },
    language: {
      type: String,
      default: 'en',
    },
  },
  lastLogin: {
    type: Date,
  },
  resetToken: {
    type: String,
  },
  resetTokenExpiry: {
    type: Date,
  },
  createdAt: {
    type: Date,
    default: Date.now,
  },
  updatedAt: {
    type: Date,
    default: Date.now,
  },
});

userSchema.pre('save', function (next) {
  this.updatedAt = new Date();
  next();
});

userSchema.index({ email: 1 });
userSchema.index({ name: 1 });
userSchema.index({ createdAt: -1 });

userSchema.methods.toPublicJSON = function () {
  return {
    id: this._id,
    email: this.email,
    name: this.name,
    bio: this.bio,
    avatar: this.avatar,
    role: this.role,
    createdAt: this.createdAt,
  };
};

const User = mongoose.model('User', userSchema);

module.exports = User;
