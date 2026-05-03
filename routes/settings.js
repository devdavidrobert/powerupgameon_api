const router = require("express").Router();
const {
  getSettings,
  updateSettings,
  clearTimers,
} = require("../controllers/settingsController");
const { authenticate } = require("../middleware/authenticate");

// Public — frontend polls this to determine game state
router.get("/", getSettings);

// Admin only
router.put("/", authenticate, updateSettings);
router.delete("/timers", authenticate, clearTimers);

module.exports = router;
