const RaffleModel = require("../models/Raffle");
const SubmissionModel = require("../models/Submission");
const { asyncHandler } = require("../utils/asyncHandler");
const { fisherYatesShuffle } = require("../utils/helpers");

/**
 * GET /api/raffles
 */
const getAllRaffles = asyncHandler(async (req, res) => {
  const raffles = await RaffleModel.findAllRaffles();
  res.json({ success: true, data: raffles });
});

/**
 * GET /api/raffles/:raffleId/winners
 */
const getRaffleWinners = asyncHandler(async (req, res) => {
  const raffle = await RaffleModel.findRaffleById(req.params.raffleId);
  if (!raffle) return res.status(404).json({ success: false, message: "Raffle not found." });

  const winners = await RaffleModel.findWinnersByRaffle(req.params.raffleId);
  res.json({ success: true, data: winners });
});

/**
 * POST /api/raffles
 * Pick winners and persist the raffle draw.
 *
 * Body: {
 *   winnerCount:    number,
 *   minScore?:      number,   // minimum score value (e.g. 8)
 *   prizeWinnersOnly?: boolean // restrict pool to subs where prize !== "Nothing"
 * }
 */
const createRaffle = asyncHandler(async (req, res) => {
  const { winnerCount, minScore = 0, prizeWinnersOnly = false } = req.body;

  if (!winnerCount || winnerCount < 1) {
    return res.status(400).json({ success: false, message: "winnerCount must be at least 1." });
  }

  let pool = await SubmissionModel.findAll();

  // Apply filters
  if (minScore > 0) pool = pool.filter((s) => s.score >= minScore);
  if (prizeWinnersOnly) pool = pool.filter((s) => s.prize && s.prize !== "Nothing");

  if (pool.length === 0) {
    return res.status(422).json({
      success: false,
      message: "No submissions match the selected criteria.",
    });
  }

  if (winnerCount > pool.length) {
    return res.status(422).json({
      success: false,
      message: `Only ${pool.length} players match the criteria. Cannot pick ${winnerCount} winners.`,
    });
  }

  const shuffled = fisherYatesShuffle([...pool]);
  const selected = shuffled.slice(0, winnerCount);

  const now = new Date();
  const raffleName = `Raffle Draw - ${now.toLocaleDateString()} ${now.toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  })}`;

  const winners = selected.map((s) => ({
    originalSubmissionId: s.id,
    fullName: s.fullName,
    prize: s.prize,
    score: s.score,
    percentage: s.percentage,
    submittedAt: s.submittedAt,
  }));

  const result = await RaffleModel.createRaffleWithWinners({ name: raffleName, winners });

  res.status(201).json({ success: true, data: result });
});

/**
 * PATCH /api/raffles/winners/:winnerId
 * Toggle giftReceived status.
 */
const updateWinnerGiftStatus = asyncHandler(async (req, res) => {
  const { giftReceived } = req.body;

  if (typeof giftReceived !== "boolean") {
    return res.status(400).json({ success: false, message: "giftReceived must be a boolean." });
  }

  await RaffleModel.updateGiftReceived(req.params.winnerId, giftReceived);
  res.json({ success: true, message: "Gift status updated." });
});

module.exports = { getAllRaffles, getRaffleWinners, createRaffle, updateWinnerGiftStatus };
