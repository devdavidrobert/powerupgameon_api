const router = require("express").Router();
const {
  getAllPrizes,
  getPrize,
  createPrize,
  updatePrize,
  deletePrize,
} = require("../controllers/prizesController");
const { authenticate } = require("../middleware/authenticate");

// Public — wheel needs the prize list
router.get("/", getAllPrizes);
router.get("/:id", getPrize);

// Admin only
router.post("/", authenticate, createPrize);
router.put("/:id", authenticate, updatePrize);
router.delete("/:id", authenticate, deletePrize);

module.exports = router;
