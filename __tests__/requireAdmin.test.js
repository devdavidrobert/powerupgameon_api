describe("requireAdmin middleware", () => {
  let requireAdmin;
  let mockRes;
  let mockNext;

  beforeEach(() => {
    jest.resetModules();
    process.env.ALLOWED_ADMIN_EMAILS = "admin@example.com";
    ({ requireAdmin } = require("../middleware/requireAdmin"));
    mockRes = { status: jest.fn().mockReturnThis(), json: jest.fn() };
    mockNext = jest.fn();
  });

  it("calls next when custom claim admin is true", () => {
    const req = { user: { uid: "1", email: "user@test.com", admin: true } };
    requireAdmin(req, mockRes, mockNext);
    expect(mockNext).toHaveBeenCalled();
    expect(mockRes.status).not.toHaveBeenCalled();
  });

  it("calls next when email is in ALLOWED_ADMIN_EMAILS", () => {
    const req = { user: { uid: "1", email: "admin@example.com", admin: false } };
    requireAdmin(req, mockRes, mockNext);
    expect(mockNext).toHaveBeenCalled();
    expect(mockRes.status).not.toHaveBeenCalled();
  });

  it("returns 403 FORBIDDEN_ADMIN when not admin and not allowlisted", () => {
    const req = { user: { uid: "1", email: "other@test.com", admin: false } };
    requireAdmin(req, mockRes, mockNext);
    expect(mockNext).not.toHaveBeenCalled();
    expect(mockRes.status).toHaveBeenCalledWith(403);
    expect(mockRes.json).toHaveBeenCalledWith(
      expect.objectContaining({ success: false, code: "FORBIDDEN_ADMIN" })
    );
  });

  it("returns 401 when req.user is missing", () => {
    const req = {};
    requireAdmin(req, mockRes, mockNext);
    expect(mockNext).not.toHaveBeenCalled();
    expect(mockRes.status).toHaveBeenCalledWith(401);
  });
});
