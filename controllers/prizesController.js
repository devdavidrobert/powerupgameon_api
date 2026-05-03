const PrizeModel = require("../models/Prize");
const { asyncHandler } = require("../utils/asyncHandler");

/**
 * GET /api/prizes
 */
const getAllPrizes = asyncHandler(async (req, res) => {
  const prizes = await PrizeModel.findAll();
  res.json({ success: true, data: prizes });
});

/**
 * GET /api/prizes/:id
 */
const getPrize = asyncHandler(async (req, res) => {
  const prize = await PrizeModel.findById(req.params.id);
  if (!prize) return res.status(404).json({ success: false, message: "Prize not found." });
  res.json({ success: true, data: prize });
});

/**
 * POST /api/prizes
 */
const createPrize = asyncHandler(async (req, res) => {
  const { name, isRealPrize, order } = req.body;

  if (!name || !name.trim()) {
    return res.status(400).json({ success: false, message: "name is required." });
  }

  const allPrizes = await PrizeModel.findAll();
  const prize = await PrizeModel.create({
    name: name.trim(),
    isRealPrize: isRealPrize !== undefined ? Boolean(isRealPrize) : true,
    order: order ?? allPrizes.length + 1,
  });

  res.status(201).json({ success: true, data: prize });
});

/**
 * PUT /api/prizes/:id
 */
const updatePrize = asyncHandler(async (req, res) => {
  const existing = await PrizeModel.findById(req.params.id);
  if (!existing) return res.status(404).json({ success: false, message: "Prize not found." });

  const { name, isRealPrize, order } = req.body;
  const updates = {};

  if (name) updates.name = name.trim();
  if (isRealPrize !== undefined) updates.isRealPrize = Boolean(isRealPrize);
  if (order !== undefined) updates.order = Number(order);

  const updated = await PrizeModel.update(req.params.id, updates);
  res.json({ success: true, data: updated });
});

/**
 * DELETE /api/prizes/:id
 */
const deletePrize = asyncHandler(async (req, res) => {
  const existing = await PrizeModel.findById(req.params.id);
  if (!existing) return res.status(404).json({ success: false, message: "Prize not found." });

  await PrizeModel.delete(req.params.id);
  res.json({ success: true, message: "Prize deleted." });
});

module.exports = { getAllPrizes, getPrize, createPrize, updatePrize, deletePrize };
