#ifdef HAVE_CONFIG_H
# include "config.h"
#endif

#include <cstdio>
#include <cstdlib>
#include <cstdarg>
#include <cstring>
#include <cassert>

#include <iostream>
#include <string>
#include <forward_list>
#include <map>
#include <set>
#include <deque>
#include <vector>

#include <dirent.h>
#include <sys/stat.h>
#include <unistd.h>

#ifdef HAVE_FNMATCH_H
# include <fnmatch.h>
#endif
#include "crc_32.h"

#define MAX_PARTIAL_SIZE (off_t)1024

#define ISFLAG(a,b) ((a & b) == b)
#define SETFLAG(a,b) (a |= b)

#define F_RECURSE           0x0001
#define F_HIDEPROGRESS      0x0002
#define F_DSAMELINE         0x0004
#define F_FOLLOWLINKS       0x0008
#define F_DELETEFILES       0x0010
#define F_EXCLUDEEMPTY      0x0020
#define F_CONSIDERHARDLINKS 0x0040
#define F_SHOWSIZE          0x0080
#define F_NOPROMPT          0x0100
off_t min_size = 0;

class crc32 {
  public:
    bool valid;
    uint32_t crc;

    crc32()
      : valid(false), crc(0)
    {}
    bool operator!=(const crc32 &other) const {
      return (valid!=other.valid || crc!=other.crc);
    }
    bool operator==(const crc32 &other) const {
      return (valid==other.valid && crc==other.crc);
    }
    crc32 &operator=(uint32_t t_crc) {
      crc = t_crc;
      valid = true;
      return *this;
    }
};

class file_t {
  public:
    std::string name;
    off_t size;
    dev_t device;
    ino_t inode;
    time_t mtime;
    bool checked;
    bool read_only;

    class crc32 crcpartial;
    class crc32 crcfull;

    file_t()
      : name(), size(0)
        , device(0), inode(0), mtime(0)
        , checked(false)
        , read_only(false)
        , crcpartial(), crcfull()
  {}
};

unsigned long flags = 0;
size_t filecount = 0;
size_t read_only_file_count = 0;

std::string program_name;
//std::forward_list<file_t> filelist;
std::map<off_t, std::deque<std::forward_list<file_t>>> filelist;
std::set<std::string> globs;

void tokenize(const std::string& str, std::vector<std::string>& tokens, const std::string& delimiters = " ", bool permit_empty=false) {
  if(permit_empty)
  {
    std::string::size_type start = 0;
    std::string::size_type end = str.find_first_of(delimiters, start);

    while(std::string::npos != start)
    {
      // Found a token, add it to the vector.
      //std::string t_str = str.substr(start, end-start);
      //fprintf(stderr, "'%s'\n", t_str.c_str() );
      tokens.push_back(str.substr(start, end-start));
      // Skip found delimiter
      start = (end==std::string::npos?end:end+1);
      // Find next delimiter
      end = str.find_first_of(delimiters, start);
    }
  }
  else
  {
    // Skip delimiters at beginning.
    std::string::size_type lastPos = str.find_first_not_of(delimiters, 0);
    // Find first "non-delimiter".
    std::string::size_type pos     = str.find_first_of(delimiters, lastPos);

    while (std::string::npos != pos || std::string::npos != lastPos)
    {
      // Found a token, add it to the vector.
      tokens.push_back(str.substr(lastPos, pos - lastPos));
      // Skip delimiters.  Note the "not_of"
      lastPos = str.find_first_not_of(delimiters, pos);
      // Find next "non-delimiter"
      pos = str.find_first_of(delimiters, lastPos);
    }
  }
}

std::set<std::string> read_only;
bool is_readonly(const std::string &path) {
  std::vector<std::string> path_parts;
  tokenize(path, path_parts, "\\/", false);
  for(auto it=path_parts.begin(); it!=path_parts.end(); ++it) {
    if(read_only.find(*it)!=read_only.end()) {
      //fprintf( stderr, "is_readonly('%s')\n", path.c_str() );
      return true;
    }
  }
  return false;
}

bool glob_include( const std::string &name ) {
  if( globs.size()==0 ) return true;
#ifdef HAVE_FNMATCH_H
  for( std::set<std::string>::const_iterator it=globs.begin(); it!=globs.end(); ++it) {
    if( fnmatch(it->c_str(), name.c_str(), 0/*FNM_PATHNAME*/)==0 ) {
      //printf( "'%s' matches '%s'.\n", name.c_str(), it->c_str() );
      return true;
    }
  }
#endif
  return false;
}

void errormsg(const char *message, ...)
{
  va_list ap;

  va_start(ap, message);

  fprintf(stderr, "\r%40s\r%s: ", "", program_name.c_str());
  vfprintf(stderr, message, ap);
}

void scandir(const std::string &dir, bool read_only)
{
  DIR *cd;
  struct dirent *dirinfo;
  int lastchar;
  struct stat info;
  struct stat linfo;
  static int progress = 0;
  static char indicator[] = "-\\|/";

  cd = opendir(dir.c_str());

  if (!cd) {
    errormsg("could not chdir to %s\n", dir.c_str());
    return;
  }

  //errormsg("Scanning %s\n", dir.c_str());

  lastchar = dir.length() - 1;
  bool add_dir_separator = false;
  if(lastchar >= 0 && dir[lastchar] != '/') {
    add_dir_separator = true;
  }
  while ((dirinfo = readdir(cd)) != NULL) {
    if (strcmp(dirinfo->d_name, ".") && strcmp(dirinfo->d_name, "..")) {
      if (!ISFLAG(flags, F_HIDEPROGRESS)) {
        fprintf(stderr, "\rBuilding file list %c ", indicator[progress]);
        progress = (progress + 1) % 4;
      }

      file_t newfile;

      newfile.name = dir;
      if (add_dir_separator) newfile.name.push_back('/');
      newfile.name += dirinfo->d_name;

      if (stat(newfile.name.c_str(), &info) == -1) {
        continue;
      }

      if (lstat(newfile.name.c_str(), &linfo) == -1) {
        continue;
      }

      newfile.size = info.st_size;
      newfile.device = info.st_dev;
      newfile.inode = info.st_ino;
      newfile.mtime = info.st_mtime;
      newfile.read_only = read_only;

      if (newfile.size == 0 && ISFLAG(flags, F_EXCLUDEEMPTY)) {
        continue;
      }

      if (S_ISDIR(info.st_mode)) {
        if (ISFLAG(flags, F_RECURSE) && (ISFLAG(flags, F_FOLLOWLINKS) || !S_ISLNK(linfo.st_mode))) {
          scandir(newfile.name, read_only || is_readonly(dirinfo->d_name));
        }
      } else {
        if ( newfile.size > min_size && (S_ISREG(linfo.st_mode) || (S_ISLNK(linfo.st_mode) && ISFLAG(flags, F_FOLLOWLINKS))) ) {
          if( !glob_include(newfile.name) ) continue;
          filelist[newfile.size].push_front(std::forward_list<file_t>(1, newfile));
          filecount ++;
          if(read_only) read_only_file_count ++;
        }
      }
    }
  }

  closedir(cd);
}

void deletefiles(bool prompt) {
  unsigned int numsets = 0;
  for(auto size_it=filelist.rbegin(); size_it!=filelist.rend(); ++size_it) {
    numsets += size_it->second.size();
  }

  unsigned int curgroup = 0;
  for(auto size_it=filelist.rbegin(); size_it!=filelist.rend(); ++size_it) {
    auto size = size_it->first;
    for(auto grp_it=size_it->second.begin(); grp_it!=size_it->second.end(); ++grp_it) {
      curgroup ++;
      unsigned int counter = 1;
      unsigned int num_ro = 0;
      std::map<unsigned int, std::string> names;
      std::map<unsigned int, bool> erase;
      for(auto file_it=grp_it->begin(); file_it!=grp_it->end(); ++file_it) {
        if(file_it->read_only) {
          num_ro ++;
        } else {
          if (prompt) printf("[%d] %s (%c)\n", counter, file_it->name.c_str(), file_it->read_only ? 'R' : 'W');
          names[counter] = file_it->name;
          erase[counter] = true;
          counter++;
        }
      }
      /* don't delete if no non-protected files */
      if (counter<=1) continue;
      if (!prompt) {
        /* preserve the first file, iff no matches in read_only */
        erase[1] = (num_ro!=0);
        for(unsigned int i=2; i<counter; i++) {
          erase[i] = true;
        }
      } else {
        printf("    %u read only.\n", num_ro);
        printf("\n");
        /* prompt for files to preserve */
        unsigned int sum = 0;
        bool done = false;
        do {
          for(unsigned int i=2; i<counter; i++) {
            erase[i] = true;
          }

          printf("Set %u of %u, preserve files [1 - %u, all, none, quit]", curgroup, numsets, counter-1);
          if (ISFLAG(flags, F_SHOWSIZE)) printf(" (%zu byte%s each)", size, (size != 1) ? "s" : "");
          printf(": ");
          fflush(stdout);

          std::string line;
          std::getline(std::cin, line);

          std::vector<std::string> tokens;
          tokenize( line, tokens, " ,\n" );
          for(auto token=tokens.begin(); token!=tokens.end(); ++token) {
            if(strcasecmp(token->c_str(), "quit")==0) {
              return;
            } else if(strcasecmp(token->c_str(), "all")==0) {
              for(unsigned int i=1; i<counter; i++) {
                erase[i] = false;
              }
              done = true;
            } else if(strcasecmp(token->c_str(), "none")==0) {
              for(unsigned int i=1; i<counter; i++) {
                erase[i] = true;
              }
              done = true;
            } else {
              unsigned int number = 0;
              sscanf(token->c_str(), "%u", &number);
              if(number > 0 && number < counter) {
                erase[number] = false;
              }
            }
          }

          sum = 0;
          unsigned int x;
          for( x=1; x<counter; x++) {
            sum += erase[x]?0:1;
          }
        } while (done==false && sum < 1); /* make sure we've preserved at least one file */
      }

      printf("\n");

      for( unsigned int x=1; x<counter; x++) { 
        if (!erase[x])
          printf( "   [+] %s\n", names[x].c_str() );
        else {
          if (remove(names[x].c_str()) == 0) {
            printf("   [-] %s\n", names[x].c_str() );
          } else {
            printf("   [!] %s ", names[x].c_str() );
            printf("-- unable to delete file!\n");
          }
        }
      }
      printf("\n");
    }
  }
}

void summarizematches(void) {
  int numsets = 0;
  double numbytes = 0.0;
  int numfiles = 0;

  for(auto size_it=filelist.rbegin(); size_it!=filelist.rend(); ++size_it) {
    for(auto grp_it=size_it->second.begin(); grp_it!=size_it->second.end(); ++grp_it) {
      numsets++;
      for(auto file_it=grp_it->begin(); file_it!=grp_it->end(); ++file_it) {
        numfiles++;
        numbytes += size_it->first;
      }
    }
  }
  if (numsets == 0) {
    printf("No duplicates found.\n\n");
  } else {
    if (numbytes < 1024.0) {
      printf("%d duplicate files (in %d sets), occupying %.0f bytes.\n\n", numfiles, numsets, numbytes);
    } else if (numbytes <= (1024.0 * 1024.0)) {
      printf("%d duplicate files (in %d sets), occupying %.1f kylobytes\n\n", numfiles, numsets, numbytes / 1024.0);
    } else {
      printf("%d duplicate files (in %d sets), occupying %.1f megabytes\n\n", numfiles, numsets, numbytes / (1024.0 * 1024.0));
    }
  }
}

void printmatches(void) {
  for(auto size_it=filelist.rbegin(); size_it!=filelist.rend(); ++size_it) {
    auto size = size_it->first;
    for(auto grp_it=size_it->second.begin(); grp_it!=size_it->second.end(); ++grp_it) {
      if (ISFLAG(flags, F_SHOWSIZE)) printf("%zu byte%s each:\n", size, (size != 1) ? "s" : "");
      for(auto file_it=grp_it->begin(); file_it!=grp_it->end(); ++file_it) {
        printf("%s (%c)%c", file_it->name.c_str(), file_it->read_only ? 'R' : 'W', ISFLAG(flags, F_DSAMELINE)?' ':'\n');
      }
      printf("\n");
    }
  }
}

void gen_partial_crc(file_t &file) {
  unsigned char buf[MAX_PARTIAL_SIZE+1];

  FILE *fp = fopen(file.name.c_str(), "r");
  if(fp==NULL) return;

  size_t size = std::min(file.size, MAX_PARTIAL_SIZE);

  uint32_t partialcrc = 0;
  while(size > 0) {
    int r = fread(buf, 1, size, fp);
    if(r>0) {
      partialcrc = crc32(partialcrc, buf, r);
    } else {
      fprintf(stderr, "Failed to read last %zu bytes from '%s'.\n", size, file.name.c_str());
      fclose(fp);
      return;
    }
    size -= r;
  }

  fclose(fp);
  file.crcpartial = partialcrc;

  if(file.size <= MAX_PARTIAL_SIZE) {
    file.crcfull = partialcrc;
  }
}

void gen_full_crc(file_t &file) {
  unsigned char buf[MAX_PARTIAL_SIZE+1];

  FILE *fp = fopen(file.name.c_str(), "r");
  if(fp==NULL) return;

  size_t size = file.size;

  uint32_t fullcrc = 0;
  while(size > 0) {
    int r = fread(buf, 1, MAX_PARTIAL_SIZE, fp);
    if(r>0) {
      fullcrc = crc32(fullcrc, buf, r);
    } else {
      fprintf(stderr, "Failed to read last %zu bytes from '%s'.\n", size, file.name.c_str());
      fclose(fp);
      return;
    }
    size -= r;
  }

  fclose(fp);
  file.crcfull = fullcrc;
}

bool byte_match(const file_t &A, const file_t &B) {
  FILE *fp_a = fopen(A.name.c_str(), "r");
  if(fp_a==NULL) {
    return false;
  }

  FILE *fp_b = fopen(B.name.c_str(), "r");
  if(fp_b==NULL) {
    fclose(fp_a);
    return false;
  }

  unsigned char buf_a[MAX_PARTIAL_SIZE+1];
  unsigned char buf_b[MAX_PARTIAL_SIZE+1];

  off_t size = A.size;

  while(size > 0) {
    int a_bytes = fread(buf_a, 1, MAX_PARTIAL_SIZE, fp_a);
    int b_bytes = fread(buf_b, 1, MAX_PARTIAL_SIZE, fp_b);

    if(a_bytes!=b_bytes) {
      /* Didn't read synchronously */
      return false;
    } else if(a_bytes>0) {
      if (memcmp (buf_a, buf_b, a_bytes)) {
        /* file contents are different */
        return false;
      }
    } else {
      /* Error reading */
      return false;
    }
    size -= a_bytes;
  }
  fclose(fp_b);
  fclose(fp_a);

  return true;
}

bool groups_match(std::forward_list<file_t> &groupA, std::forward_list<file_t> &groupB) {
  file_t &fileA = *groupA.begin();
  file_t &fileB = *groupB.begin();

  assert(fileA.size==fileB.size);
  if(fileA.size==0) return true;

  if(!fileA.crcpartial.valid) gen_partial_crc(fileA);
  if(!fileB.crcpartial.valid) gen_partial_crc(fileB);
  if(!fileA.crcpartial.valid || !fileB.crcpartial.valid || fileA.crcpartial != fileB.crcpartial) {
    //fprintf(stderr, "A: '%s' - partial CRC %u - Valid %s\n", fileA.name.c_str(), fileA.crcpartial.crc, fileA.crcpartial.valid?"TRUE":"FALSE");
    //fprintf(stderr, "B: '%s' - partial CRC %u - Valid %s\n", fileB.name.c_str(), fileB.crcpartial.crc, fileB.crcpartial.valid?"TRUE":"FALSE");
    return false;
  }

  if(!fileA.crcfull.valid) gen_full_crc(fileA);
  if(!fileB.crcfull.valid) gen_full_crc(fileB);
  if(fileA.crcfull != fileB.crcfull) {
    //fprintf(stderr, "A: '%s' - full CRC %u - Valid %s\n", fileA.name.c_str(), fileA.crcfull.crc, fileA.crcfull.valid?"TRUE":"FALSE");
    //fprintf(stderr, "B: '%s' - full CRC %u - Valid %s\n", fileB.name.c_str(), fileB.crcfull.crc, fileB.crcfull.valid?"TRUE":"FALSE");
    return false;
  }

  if(byte_match(fileA, fileB)) return true;
  return false;
}

void build_matches() {
  size_t progress = 0;

  std::map<off_t, std::deque<std::forward_list<file_t>>> next_filelist;
  for(auto size_it=filelist.rbegin(); size_it!=filelist.rend(); ++size_it) {
    if(size_it->second.size()<=1) {
      progress += size_it->second.size();
      continue;
    }

    //fprintf(stderr, "\r%40sSize: %zu Groups: %zu%40s", "", size_it->first, size_it->second.size(), "");
    // Populate queue
    std::deque<std::forward_list<file_t>> queue;
    for(auto grp_it=size_it->second.begin(); grp_it!=size_it->second.end(); ++grp_it) {
      queue.push_back(*grp_it);
    }

    while(queue.size()>0) {
      auto grp_A_it = queue.begin();
      std::deque<std::forward_list<file_t>> next_queue;
      std::forward_list<file_t> cur_group = *grp_A_it;
      for(auto grp_B_it=grp_A_it+1; grp_B_it!=queue.end(); ++grp_B_it) {
        if(groups_match(cur_group, *grp_B_it)) {
          cur_group.insert_after ( cur_group.before_begin(), grp_B_it->begin(), grp_B_it->end() );
          progress++;
        } else {
          next_queue.push_back(*grp_B_it);
        }
      }
      if (!ISFLAG(flags, F_HIDEPROGRESS)) {
        fprintf(stderr, "\rProgress [%zu/%zu] (size %zu) %d%% ", progress, filecount, size_it->first, (int)((float) progress / (float) filecount * 100.0));
        progress++;
      }
      size_t cg_size = 0;
      for(auto cg_it=cur_group.begin(); cg_it!=cur_group.end(); ++cg_it) {
        cg_size++;
      }
      if(cg_size>1) {
        next_filelist[size_it->first].push_back(cur_group);
      }
      queue = next_queue;
    }
  }
  if (!ISFLAG(flags, F_HIDEPROGRESS)) fprintf(stderr, "\r%40s\r", " ");
  filelist = next_filelist;
}

void dump_filelist() {
  off_t group_id = 0;
  for(auto size_it=filelist.rbegin(); size_it!=filelist.rend(); ++size_it) {
    for(auto grp_it=size_it->second.begin(); grp_it!=size_it->second.end(); ++grp_it) {
      group_id++;
      for(auto file_it=grp_it->begin(); file_it!=grp_it->end(); ++file_it) {
        fprintf( stderr, "\n%zu\t'%s'\t%zu", group_id, file_it->name.c_str(), file_it->size );
      }
    }
  }
  fprintf( stderr, "\n" );
}

void help_text()
{
  printf("Usage: fdupes [options] DIRECTORY...\n\n");
  printf("TODO: Switch to ogs getopt\n");
  printf("(for option->help synchronization).\n");

  printf(" -r\tfor every directory given follow subdirectories\n");
  printf("   \tencountered within\n");
  printf(" -R name\tany directory with at least one component\n");
  printf("   \tmatching 'name' should be treated as read only\n");
  printf(" -i glob\tonly include files matching 'glob'; multiple\n");
  printf("   \tinstances, files must match at least one 'glob'\n");
  printf(" -s\tfollow symlinks\n");
  printf(" -n\texclude zero-length files from consideration\n");
  printf(" -f\tomit the first file in each set of matches\n");
  printf(" -1\tlist each set of matches on a single line\n");
  printf(" -S\tshow size of duplicate files\n");
  printf(" -m\tsummarize dupe information\n");
  printf(" -M min\tOnly process files of size at least 'min' bytes\n");
  printf(" -q\thide progress indicator\n");
  printf(" -d\tprompt user for files to preserve and delete all\n"); 
  printf("   \tothers; important: under particular circumstances,\n");
  printf("   \tdata may be lost when using this option together\n");
  printf("   \twith -s or --symlinks, or when specifying a\n");
  printf("   \tparticular directory more than once; refer to the\n");
  printf("   \tfdupes documentation for additional information\n");
  printf(" -N\ttogether with --delete, preserve the first file in\n");
  printf("   \teach set of duplicates and delete the rest without\n");
  printf("   \twithout prompting the user\n");
  printf(" -v\tdisplay fdupes version\n");
  printf(" -h\tdisplay this help message\n\n");
}

int main(int argc, char *argv[]) {
  program_name = argv[0];

  int opt;
  while ((opt = getopt(argc, argv, "rq1SsndvhNM:R:i:")) != EOF) {
    switch (opt) {
      case 'r':
        SETFLAG(flags, F_RECURSE);
        break;
      case 'q':
        SETFLAG(flags, F_HIDEPROGRESS);
        break;
      case '1':
        SETFLAG(flags, F_DSAMELINE);
        break;
      case 'S':
        SETFLAG(flags, F_SHOWSIZE);
        break;
      case 's':
        SETFLAG(flags, F_FOLLOWLINKS);
        break;
      case 'n':
        SETFLAG(flags, F_EXCLUDEEMPTY);
        break;
      case 'd':
        SETFLAG(flags, F_DELETEFILES);
        break;
      case 'v':
        printf("fdupes %s\n", VERSION);
        exit(0);
      case 'h':
        help_text();
        exit(1);
      case 'N':
        SETFLAG(flags, F_NOPROMPT);
        break;
      case 'M':
	min_size = atol(optarg);
	break;
      case 'R':
        read_only.insert(optarg);
        break;
      case 'i':
#ifdef HAVE_FNMATCH_H
        globs.insert(optarg);
#else
        printf("WARNING: Not compiled with glob support. Ignoring inclusion on '%s'.\n", optarg);
#endif
        break;

      default:
        fprintf(stderr, "Try `fdupes -h' for more information.\n");
        exit(1);
    }
  }

  if (optind >= argc) {
    errormsg("no directories specified\n");
    exit(1);
  }

  if(min_size != 0) {
    printf( "minimum file size to consider: %zu\n", min_size );
  }
  for (int x = optind; x < argc; x++) {
    scandir( argv[x], is_readonly(argv[x]) );
  }
  if(read_only.size()>0) {
    printf("Read only paths: ");
    for (auto it=read_only.begin(); it!=read_only.end(); ++it) {
      printf("'%s' ", it->c_str() );
    }
    printf("\n");
    printf("Total read only files: %zu.\n", read_only_file_count);
  }

  //dump_filelist();
  build_matches();
  if (ISFLAG(flags, F_DELETEFILES)) {
    if (ISFLAG(flags, F_NOPROMPT)) {
      deletefiles(false);
    } else {
      deletefiles(true);
    }
  } else {
    printmatches();
  }

  return 0;
}

