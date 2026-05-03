const express = require("express");
const cors = require("cors");
const helmet = require("helmet");
const rateLimit = require("express-rate-limit");

const { allowedOrigins, nodeEnv, port, apiCsrfSecret } = require("./config/env");
const { requestContext } = require("./middleware/requestContext");
const { requireCsrfToken, mintCsrfToken } = require("./middleware/csrf");
const { log } = require("./utils/logger");

const questionsRouter = require("./routes/questions");
const prizesRouter = require("./routes/prizes");
const registrationsRouter = require("./routes/registrations");
const submissionsRouter = require("./routes/submissions");
const settingsRouter = require("./routes/settings");
const rafflesRouter = require("./routes/raffles");
const authRouter = require("./routes/auth");
const spinRouter = require("./routes/spin");

const { errorHandler } = require("./middleware/errorHandler");

if (nodeEnv === "production" && !apiCsrfSecret) {
  throw new Error("API_CSRF_SECRET must be set in production.");
}

const app = express();

app.use(helmet());

app.use(
  cors({
    origin(origin, callback) {
      if (!origin) return callback(null, true);
      if (allowedOrigins.includes(origin)) return callback(null, true);
      log("warn", "cors_rejected", { origin });
      return callback(null, false);
    },
    credentials: true,
    allowedHeaders: ["Content-Type", "Authorization", "X-CSRF-Token", "X-Request-Id"],
  })
);

app.use(express.json({ limit: "256kb" }));
app.use(express.urlencoded({ extended: true }));
app.use(requestContext);

const globalLimiter = rateLimit({
  windowMs: 15 * 60 * 1000,
  max: 200,
  standardHeaders: true,
  legacyHeaders: false,
  message: { success: false, message: "Too many requests. Please try again later." },
});
app.use(globalLimiter);

app.get("/api/csrf-token", (req, res) => {
  res.json({ success: true, data: { csrfToken: mintCsrfToken() } });
});

app.get("/health", (req, res) => {
  res.json({ status: "ok", timestamp: new Date().toISOString() });
});

app.use("/api/auth", requireCsrfToken, authRouter);
app.use("/api/questions", requireCsrfToken, questionsRouter);
app.use("/api/prizes", requireCsrfToken, prizesRouter);
app.use("/api/registrations", requireCsrfToken, registrationsRouter);
app.use("/api/submissions", requireCsrfToken, submissionsRouter);
app.use("/api/spin", requireCsrfToken, spinRouter);
app.use("/api/settings", requireCsrfToken, settingsRouter);
app.use("/api/raffles", requireCsrfToken, rafflesRouter);

app.use((req, res) => {
  res.status(404).json({ success: false, message: "Route not found." });
});

app.use(errorHandler);

const PORT = port || 4000;

if (require.main === module) {
  app.listen(PORT, () => {
    log("info", "api_listen", { port: PORT, nodeEnv });
  });
}

module.exports = app;
