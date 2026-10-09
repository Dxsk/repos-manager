const htmlmin = require("html-minifier-terser");
const CleanCSS = require("clean-css");
const syntaxHighlight = require("@11ty/eleventy-plugin-syntaxhighlight");
const fs = require("fs");
const path = require("path");
const isProd = process.env.NODE_ENV === "production";

module.exports = function(eleventyConfig) {
  eleventyConfig.addPlugin(syntaxHighlight);
  eleventyConfig.addPassthroughCopy("src/assets/img");
  eleventyConfig.addPassthroughCopy("src/CNAME");
  // Google Search Console ownership file, served byte for byte.
  eleventyConfig.addPassthroughCopy("src/google1e3ba38a02f931fb.html");
  eleventyConfig.ignores.add("src/google1e3ba38a02f931fb.html");

  eleventyConfig.addFilter("isoDate", (d) => new Date(d).toISOString().slice(0, 10));
  // Stylesheets are small, so they are inlined in each page: no render-blocking
  // request, and no stale cached CSS after a deploy (GitHub Pages caches 10 min).
  const cssCache = new Map();
  eleventyConfig.addFilter("inlineCss", (url) => {
    if (isProd && cssCache.has(url)) return cssCache.get(url);
    const css = fs.readFileSync(path.join(__dirname, "src", url), "utf8");
    const out = isProd ? new CleanCSS({}).minify(css).styles : css;
    if (isProd) cssCache.set(url, out);
    return out;
  });
  eleventyConfig.addWatchTarget("src/assets/css/");

  eleventyConfig.addCollection("sortedDocs", function(collectionApi) {
    return collectionApi.getFilteredByTag("docs").sort((a, b) => {
      return (a.data.order || 0) - (b.data.order || 0);
    });
  });

  if (isProd) {
    // Minify HTML
    eleventyConfig.addTransform("htmlmin", async function(content) {
      if ((this.page.outputPath || "").endsWith(".html")) {
        return await htmlmin.minify(content, {
          collapseWhitespace: true,
          conservativeCollapse: true,
          removeComments: true,
          minifyCSS: true,
          minifyJS: true,
          ignoreCustomFragments: [/<%[\s\S]*?%>/, /<pre[\s\S]*?<\/pre>/]
        });
      }
      return content;
    });
  }

  return {
    dir: { input: "src", output: "_site", includes: "_includes", data: "_data" },
    templateFormats: ["njk", "md", "html"],
    markdownTemplateEngine: "njk",
    htmlTemplateEngine: "njk"
  };
};
