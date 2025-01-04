export default {
  id: "build-backend",
  name: "Backend",
  description: "Backend plugin",
  version: "1.0.0",
  author: {
    name: "John Doe",
    email: "john.doe@example.com",
    url: "https://example.com",
  },
  plugins: [
    {
      kind: "backend",
      id: "backend",
      root: "./packages/backend",
    }
  ],
};