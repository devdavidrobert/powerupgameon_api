const router = require("express").Router();
const {
  getAllPrizes,
  getPrize,
  createPrize,
  updatePrize,
  deletePrize,
} = require("../controllers/prizesController");
const { authenticate } = require("../middleware/authenticate");
const { requireAdmin } = require("../middleware/requireAdmin");

// Public — wheel needs the prize list
router.get("/", getAllPrizes);
router.get("/:id", getPrize);

// Admin only
router.post("/", authenticate, requireAdmin, createPrize);
router.put("/:id", authenticate, requireAdmin, updatePrize);
router.delete("/:id", authenticate, requireAdmin, deletePrize);

module.exports = router;
