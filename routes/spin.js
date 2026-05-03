const router = require("express").Router();
const { spinWheel } = require("../controllers/spinController");
const { spinLimiter } = require("../middleware/rateLimiters");

router.post("/", spinLimiter, spinWheel);

module.exports = router;
