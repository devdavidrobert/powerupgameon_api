const requestLogger = (req, res, next) => {
  const start = Date.now();

  res.on("finish", () => {
    const duration = Date.now() - start;
    const color =
      res.statusCode >= 500
        ? "\x1b[31m" // red
        : res.statusCode >= 400
        ? "\x1b[33m" // yellow
        : "\x1b[32m"; // green
    const reset = "\x1b[0m";

    console.log(
      `${color}[${res.statusCode}]${reset} ${req.method} ${req.originalUrl} — ${duration}ms`
    );
  });

  next();
};

module.exports = { requestLogger };
