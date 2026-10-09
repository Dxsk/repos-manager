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

    // Minify CSS after build
    eleventyConfig.on("eleventy.after", () => {
      const cssDir = path.join(__dirname, "_site", "assets", "css");
      if (!fs.existsSync(cssDir)) {
        fs.mkdirSync(cssDir, { recursive: true });
      }
      for (const file of ["prism.css", "style.css"]) {
        const src = path.join(__dirname, "src", "assets", "css", file);
        if (fs.existsSync(src)) {
          const content = fs.readFileSync(src, "utf8");
          const minified = new CleanCSS({}).minify(content).styles;
          fs.writeFileSync(path.join(cssDir, file), minified);
        }
      }
    });
  } else {
    eleventyConfig.addPassthroughCopy("src/assets/css");
  }

  return {
    dir: { input: "src", output: "_site", includes: "_includes", data: "_data" },
    templateFormats: ["njk", "md", "html"],
    markdownTemplateEngine: "njk",
    htmlTemplateEngine: "njk"
  };
};
