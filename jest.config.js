module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  roots: ['<rootDir>/tests'],
  transform: {
    '^.+\\.ts$': ['ts-jest', { tsconfig: 'bot/tsconfig.json' }],
  },
  moduleFileExtensions: ['ts', 'js', 'json'],
};
