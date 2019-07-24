#ifndef CRC_32_H
#define CRC_32_H

#include <cstdlib>
#include <cstdint>

uint32_t crc32(uint32_t crc, const void *buf, size_t size);

#endif//CRC_32_H
