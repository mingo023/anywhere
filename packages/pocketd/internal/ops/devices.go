package ops

func (s *Server) deviceOp(m Msg) Msg {
	if m.Op == "devices" {
		return Msg{Ev: "devices", Devices: s.Devices.List()}
	}
	id, err := s.Devices.Resolve(m.ID)
	if err != nil {
		return refused(m.ID, err)
	}
	if m.Op == "devices.rename" {
		d, err := s.Devices.Rename(id, m.Text)
		if err != nil {
			return refused(m.ID, err)
		}
		return Msg{Ev: "ok", ID: id, Text: d.Name}
	}
	d, err := s.Devices.Revoke(id)
	if err != nil {
		return refused(m.ID, err)
	}
	s.Kick(id, "revoked")
	return Msg{Ev: "ok", ID: id, Text: d.Name}
}
