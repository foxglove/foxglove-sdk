module.exports = {
  webpack: (config) => {
    config.module.rules.push({
      test: /\.wasm$/i,
      type: "asset/inline",
    });
    return config;
  },
};
