export default {
  id: "build-frontend",
  name: "Frontend",
  description: "Frontend plugin",
  version: "1.0.0",
  author: {
    name: "John Doe",
    email: "john.doe@example.com",
    url: "https://example.com",
  },
  plugins: [
    {
      kind: "frontend",
      id: "frontend",
      root: "./packages/frontend",
    }
  ],
};