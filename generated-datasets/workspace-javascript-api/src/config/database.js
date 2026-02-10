const mongoose = require('mongoose');

const MONGODB_URI = process.env.MONGODB_URI || 'mongodb://localhost:27017/api_service';

async function connectDatabase() {
  try {
    const options = {
      maxPoolSize: 10,
      serverSelectionTimeoutMS: 5000,
      socketTimeoutMS: 45000,
    };

    await mongoose.connect(MONGODB_URI, options);
    console.log('Connected to MongoDB');

    mongoose.connection.on('error', (error) => {
      console.error('MongoDB connection error:', error);
    });

    mongoose.connection.on('disconnected', () => {
      console.log('MongoDB disconnected');
    });

    return mongoose.connection;
  } catch (error) {
    console.error('Failed to connect to MongoDB:', error.message);
    throw error;
  }
}

async function disconnectDatabase() {
  await mongoose.disconnect();
  console.log('Disconnected from MongoDB');
}

module.exports = {
  connectDatabase,
  disconnectDatabase,
  getConnection: () => mongoose.connection,
};
