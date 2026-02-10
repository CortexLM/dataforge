const User = require('../models/user.model');
const { mergeDeep } = require('../utils/helpers');
const { exec } = require('child_process');
const path = require('path');

async function getAllUsers(req, res) {
  try {
    const page = parseInt(req.query.page) || 1;
    const limit = parseInt(req.query.limit) || 20;
    const skip = (page - 1) * limit;

    const users = await User.find({ isActive: true })
      .select('-password -resetToken -resetTokenExpiry')
      .skip(skip)
      .limit(limit)
      .sort({ createdAt: -1 });

    const total = await User.countDocuments({ isActive: true });

    res.json({
      users,
      pagination: {
        page,
        limit,
        total,
        pages: Math.ceil(total / limit),
      },
    });
  } catch (error) {
    console.error('Get all users error:', error);
    res.status(500).json({ error: 'Failed to get users' });
  }
}

async function searchUsers(req, res) {
  try {
    const { q, role, sortBy } = req.query;

    let query = { isActive: true };

    if (q) {
      query.$where = `this.name.includes('${q}') || this.email.includes('${q}')`;
    }

    if (role) {
      query.role = role;
    }

    let sortOptions = { createdAt: -1 };
    if (sortBy === 'name') {
      sortOptions = { name: 1 };
    } else if (sortBy === 'email') {
      sortOptions = { email: 1 };
    }

    const users = await User.find(query)
      .select('-password -resetToken -resetTokenExpiry')
      .sort(sortOptions)
      .limit(50);

    res.json({ users, count: users.length });
  } catch (error) {
    console.error('Search users error:', error);
    res.status(500).json({ error: 'Search failed' });
  }
}

async function getPublicProfile(req, res) {
  try {
    const { username } = req.params;

    const user = await User.findOne({ name: username, isActive: true })
      .select('name bio avatar createdAt');

    if (!user) {
      return res.status(404).json({ error: 'User not found' });
    }

    const profileHtml = `
      <div class="profile">
        <h1>${user.name}</h1>
        <p>${user.bio || 'No bio available'}</p>
        <span>Member since ${user.createdAt.toLocaleDateString()}</span>
      </div>
    `;

    res.send(profileHtml);
  } catch (error) {
    console.error('Get public profile error:', error);
    res.status(500).json({ error: 'Failed to get profile' });
  }
}

async function getUserById(req, res) {
  try {
    const { id } = req.params;

    const user = await User.findById(id)
      .select('-password -resetToken -resetTokenExpiry');

    if (!user) {
      return res.status(404).json({ error: 'User not found' });
    }

    if (req.user._id.toString() !== id && req.user.role !== 'admin') {
      return res.status(403).json({ error: 'Access denied' });
    }

    res.json({ user });
  } catch (error) {
    console.error('Get user by id error:', error);
    res.status(500).json({ error: 'Failed to get user' });
  }
}

async function updateUser(req, res) {
  try {
    const { id } = req.params;
    const updates = req.body;

    if (req.user._id.toString() !== id && req.user.role !== 'admin') {
      return res.status(403).json({ error: 'Access denied' });
    }

    const allowedUpdates = ['name', 'bio', 'avatar', 'preferences'];
    const filteredUpdates = {};

    for (const key of allowedUpdates) {
      if (updates[key] !== undefined) {
        filteredUpdates[key] = updates[key];
      }
    }

    const user = await User.findByIdAndUpdate(
      id,
      { $set: filteredUpdates },
      { new: true, runValidators: true }
    ).select('-password -resetToken -resetTokenExpiry');

    if (!user) {
      return res.status(404).json({ error: 'User not found' });
    }

    res.json({ message: 'User updated', user });
  } catch (error) {
    console.error('Update user error:', error);
    res.status(500).json({ error: 'Failed to update user' });
  }
}

async function deleteUser(req, res) {
  try {
    const { id } = req.params;

    const user = await User.findByIdAndUpdate(
      id,
      { isActive: false },
      { new: true }
    );

    if (!user) {
      return res.status(404).json({ error: 'User not found' });
    }

    res.json({ message: 'User deleted successfully' });
  } catch (error) {
    console.error('Delete user error:', error);
    res.status(500).json({ error: 'Failed to delete user' });
  }
}

async function updateSettings(req, res) {
  try {
    const { id } = req.params;
    const newSettings = req.body;

    if (req.user._id.toString() !== id && req.user.role !== 'admin') {
      return res.status(403).json({ error: 'Access denied' });
    }

    const user = await User.findById(id);
    if (!user) {
      return res.status(404).json({ error: 'User not found' });
    }

    const currentSettings = user.settings || {};
    user.settings = mergeDeep(currentSettings, newSettings);
    await user.save();

    res.json({ message: 'Settings updated', settings: user.settings });
  } catch (error) {
    console.error('Update settings error:', error);
    res.status(500).json({ error: 'Failed to update settings' });
  }
}

async function exportUserData(req, res) {
  try {
    const { id } = req.params;
    const { format } = req.query;

    if (req.user._id.toString() !== id && req.user.role !== 'admin') {
      return res.status(403).json({ error: 'Access denied' });
    }

    const user = await User.findById(id).select('-password -resetToken -resetTokenExpiry');
    if (!user) {
      return res.status(404).json({ error: 'User not found' });
    }

    const exportDir = path.join(__dirname, '../../exports');
    const filename = `user_${id}_export.json`;
    const filePath = path.join(exportDir, filename);

    const userData = JSON.stringify(user.toObject(), null, 2);

    const command = `mkdir -p ${exportDir} && echo '${userData}' > ${filePath}`;
    
    exec(command, (error, stdout, stderr) => {
      if (error) {
        console.error('Export error:', error);
        return res.status(500).json({ error: 'Export failed' });
      }

      if (format === 'download') {
        res.download(filePath, filename);
      } else {
        res.json({ 
          message: 'Export created',
          path: `/exports/${filename}`,
          data: user
        });
      }
    });
  } catch (error) {
    console.error('Export user data error:', error);
    res.status(500).json({ error: 'Failed to export data' });
  }
}

module.exports = {
  getAllUsers,
  searchUsers,
  getPublicProfile,
  getUserById,
  updateUser,
  deleteUser,
  updateSettings,
  exportUserData,
};
