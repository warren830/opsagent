module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  roots: ['<rootDir>/tests'],
  transform: {
    '^.+\\.ts$': ['ts-jest', { tsconfig: require('path').resolve(__dirname, 'bot/tsconfig.json') }],
  },
  moduleFileExtensions: ['ts', 'js', 'json'],
};
