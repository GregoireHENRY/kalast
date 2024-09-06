// From JB Vincent DLR

#include <stdio.h>
#include <stdlib.h>

int main(int argc, char* argv[])
{
	if(argc < 3){
		printf("Usage: csv2bin INPUT OUTPUT\n");
		return 1;
	}

	FILE* fid = fopen(argv[1], "r");
	if(fid==NULL){
		printf("Error: %s could not be open\n", argv[1]);
		return 1;
	}

	FILE* out = fopen(argv[2], "wb");

	char hdr[256];
	float val;
	fscanf(fid, "%s", hdr); // header row

	int i = 0;
	while(fscanf(fid, "%f\n", &val) != EOF){
		fwrite(&val, sizeof(float), 1, out);
		i++;
	}

	fclose(out);
	fclose(fid);

	printf("Wrote %i values to %s (%i bytes)\n", i, argv[2], i*4);

    return 0;
}
