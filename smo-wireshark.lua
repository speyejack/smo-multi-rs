smo_proto = Proto("smo", "Super mario odyssy online protocol")

function smo_proto.dissector(buffer, pinfo, tree)
   local proto = pinfo.port_type
   pinfo.cols.protocol = "SMOO"
   local subtree = tree:add(smo_proto, buffer(), "SMO Protocol Data")
   local type_id = buffer(16,2):le_uint()
   local type_name = "Unknown packet"
   if type_id > 0 and type_id < 15 then
	  type_name = packet_names[type_id]
   end
   subtree:add(buffer(0,16), "The UID: " .. buffer(0,16))
   subtree:add_le(buffer(16,2), "Packet type: " .. type_name .. "(" .. type_id .. ")")
   subtree:add_le(buffer(18,2), "Data size: " .. buffer(18,2):le_uint())
   if type_id == 2 then
	  subtree:add_le(buffer(20,4), "Vec x: " .. buffer(20,4):le_float())
	  subtree:add_le(buffer(24,4), "Vec y: " .. buffer(24,4):le_float())
	  subtree:add_le(buffer(28,4), "Vec z: " .. buffer(28,4):le_float())
	  subtree:add_le(buffer(72,2), "Act: " .. buffer(72, 2):le_uint())
	  subtree:add_le(buffer(74,2), "SubAct: " .. buffer(74, 2):le_uint())

   end
end

packet_names = {"Init", "Player", "Cap", "Game", "Tag", "Connect", "Disconnect", "Costume", "Shine", "Capture", "ChangeStage", "Command", "UdpInit", "HolePunch"}

tcp_table = DissectorTable.get("tcp.port")
tcp_table:add(1027, smo_proto)

udp_table = DissectorTable.get("udp.port")
udp_table:add(41553, smo_proto)
udp_table:add(41554, smo_proto)
