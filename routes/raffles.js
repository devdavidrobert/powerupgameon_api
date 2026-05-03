const router = require("express").Router();
const {
  getAllRaffles,
  getRaffleWinners,
  createRaffle,
  updateWinnerGiftStatus,
} = require("../controllers/rafflesController");
const { authenticate } = require("../middleware/authenticate");

// All raffle endpoints are admin-only
router.use(authenticate);

router.get("/", getAllRaffles);
router.post("/", createRaffle);
router.get("/:raffleId/winners", getRaffleWinners);
router.patch("/winners/:winnerId", updateWinnerGiftStatus);

module.exports = router;
