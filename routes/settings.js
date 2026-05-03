const router = require("express").Router();
const {
  getSettings,
  updateSettings,
  clearTimers,
} = require("../controllers/settingsController");
const { authenticate } = require("../middleware/authenticate");
const { requireAdmin } = require("../middleware/requireAdmin");

// Public — frontend polls this to determine game state
router.get("/", getSettings);

// Admin only
router.put("/", authenticate, requireAdmin, updateSettings);
router.delete("/timers", authenticate, requireAdmin, clearTimers);

module.exports = router;
