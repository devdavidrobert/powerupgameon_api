const router = require("express").Router();
const {
  getAllRaffles,
  getRaffleWinners,
  createRaffle,
  updateWinnerGiftStatus,
} = require("../controllers/rafflesController");
const { authenticate } = require("../middleware/authenticate");
const { requireAdmin } = require("../middleware/requireAdmin");

// All raffle endpoints are admin-only
router.use(authenticate);
router.use(requireAdmin);

router.get("/", getAllRaffles);
router.post("/", createRaffle);
router.get("/:raffleId/winners", getRaffleWinners);
router.patch("/winners/:winnerId", updateWinnerGiftStatus);

module.exports = router;
