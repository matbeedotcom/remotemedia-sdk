/** @type {import('jest').Config} */
module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  testMatch: ['**/*.test.ts'],
  moduleFileExtensions: ['ts', 'js', 'json'],
  rootDir: '.',
  verbose: true,
  testTimeout: 30000,
  // Transform TypeScript
  transform: {
    '^.+\\.tsx?$': ['ts-jest', {
      useESM: true,
      tsconfig: {
        module: 'CommonJS',
        moduleResolution: 'Node',
        esModuleInterop: true,
        target: 'ES2020',
      },
    }],
  },
};
