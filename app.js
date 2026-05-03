const express = require("express");
const cors = require("cors");
const helmet = require("helmet");
const rateLimit = require("express-rate-limit");

const questionsRouter = require("./routes/questions");
const prizesRouter = require("./routes/prizes");
const registrationsRouter = require("./routes/registrations");
const submissionsRouter = require("./routes/submissions");
const settingsRouter = require("./routes/settings");
const rafflesRouter = require("./routes/raffles");
const authRouter = require("./routes/auth");

const { errorHandler } = require("./middleware/errorHandler");
const { requestLogger } = require("./middleware/requestLogger");

const app = express();

// ── Security & Parsing ──────────────────────────────────────────────
app.use(helmet());
app.use(cors({ origin: process.env.ALLOWED_ORIGINS?.split(",") || "*" }));
app.use(express.json());
app.use(express.urlencoded({ extended: true }));

// ── Logging ──────────────────────────────────────────────────────────
app.use(requestLogger);

// ── Global Rate Limiter ──────────────────────────────────────────────
const globalLimiter = rateLimit({
  windowMs: 15 * 60 * 1000, // 15 minutes
  max: 200,
  standardHeaders: true,
  legacyHeaders: false,
  message: { success: false, message: "Too many requests. Please try again later." },
});
app.use(globalLimiter);

// ── Health Check ─────────────────────────────────────────────────────
app.get("/health", (req, res) => {
  res.json({ status: "ok", timestamp: new Date().toISOString() });
});

// ── API Routes ───────────────────────────────────────────────────────
app.use("/api/auth", authRouter);
app.use("/api/questions", questionsRouter);
app.use("/api/prizes", prizesRouter);
app.use("/api/registrations", registrationsRouter);
app.use("/api/submissions", submissionsRouter);
app.use("/api/settings", settingsRouter);
app.use("/api/raffles", rafflesRouter);

// ── 404 Handler ──────────────────────────────────────────────────────
app.use((req, res) => {
  res.status(404).json({ success: false, message: "Route not found." });
});

// ── Global Error Handler ─────────────────────────────────────────────
app.use(errorHandler);

const PORT = process.env.PORT || 4000;
app.listen(PORT, () => {
  console.log(`🚀 Steam API running on port ${PORT}`);
});

module.exports = app;
