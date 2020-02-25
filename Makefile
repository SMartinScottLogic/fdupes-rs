fdupes: old_src/fdupes.cpp old_src/crc_32.cpp old_src/crc_32.h
	g++ -DVERSION=1 old_src/fdupes.cpp old_src/crc_32.cpp -o ./fdupes
