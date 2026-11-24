Page({
  data: {
    title: "Hello, Mini Program Runtime"
  },
  onLoad() {
    this.setData({ loaded: true });
    return this.data.loaded;
  }
});
