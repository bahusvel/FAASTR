package main

import (
	"bytes"
	"debug/elf"
	"encoding/json"
	"fmt"
	"io"
	"io/ioutil"
	"log"
	"os"
	"os/exec"
	"path"
	"strings"

	"github.com/urfave/cli"
)

type Manifest struct {
	ModuleName string
	//Architecture string
	//TextSize     uint64
	//RoDataAddr   uint64
	//RoDataSize   uint64
	SymbolTable SymbolTable
}

type SymbolTable []SymbolTableEntry

type ABI int
type Visibility int

const (
	C ABI = iota
	SOS
)

const (
	Public Visibility = iota
	Private
)

type SymbolTableEntry struct {
	Name       string
	Offset     uint64
	ABI        ABI
	Visibility Visibility
}

func (this Manifest) Serialize() []byte {
	data, err := json.Marshal(this)
	if err != nil {
		panic(err)
	}
	return data
}

func Deserialize(data []byte) (Manifest, error) {
	manifest := Manifest{}
	err := json.Unmarshal(data, &manifest)
	return manifest, err
}

const (
	LLC   = "llc"
	DEBUG = true

	LINKER_MERGE = `call = 0x700000;
	SECTIONS
	{
	. = 0x800000;
	.text : {*(.text)}
	.rodata ALIGN(0x1000): {*(.rodata .rodata.* .data .data.*)}
	}`
	LINKER_NOMERGE = `call = 0x700000;
	SECTIONS
	{
	. = 0x800000;
	.text : {*(.text)}
	.rodata ALIGN(0x1000): {*(.rodata .rodata.*)}
	}`
)

var (
	LinkerFile     *os.File
	Stdlls         []string
	Architechtures = []string{"x86_64"}
	CompilerFlags  = map[string][]string{"x86_64": []string{"-mtriple=x86_64-none-gnu"}, "arm": []string{"-mtriple=arm-none-gnueabihf"}}

	// Flags
	OutputLinkedLL bool
	MergeData      bool
	InjectManifest bool
	StdPath        string
	PassPath       string
)

func replaceExtension(filename string, extension string) string {
	i := strings.LastIndex(filename, ".")
	if i > 0 {
		return filename[:i] + extension
	}
	return filename
}

func listStdLib() ([]string, error) {
	infos, err := ioutil.ReadDir(StdPath)
	if err != nil {
		return []string{}, err
	}
	lls := []string{}
	for _, info := range infos {
		if strings.HasSuffix(info.Name(), ".ll") {
			lls = append(lls, StdPath+"/"+info.Name())
		}
	}
	return lls, nil
}

func injectManifest(binary *os.File, manifest *Manifest) error {
	manData := manifest.Serialize()

	manFile, err := ioutil.TempFile("", "manifest.json")
	if err != nil {
		return err
	}
	defer os.Remove(manFile.Name())

	_, err = manFile.Write(manData)
	if err != nil {
		return err
	}

	err = manFile.Close()
	if err != nil {
		return err
	}

	cmd := exec.Command("objcopy", "--add-section", ".manifest="+manFile.Name(), binary.Name(), binary.Name())

	errBuf := &bytes.Buffer{}
	cmd.Stderr = errBuf

	err = cmd.Run()
	if err != nil {
		return fmt.Errorf("%v %s", err, string(errBuf.Bytes()))
	}

	return nil
}

func parseAnnotationPass(data []byte) (SymbolTable, error) {
	table := SymbolTable{}

	lines := strings.Split(string(data), "\n")

	for _, line := range lines {
		if line == "" {
			continue
		}

		parts := strings.Split(line, ":")
		if len(parts) != 2 {
			return table, fmt.Errorf("Annotation pass returned data in invalid format - lines")
		}
		var vis Visibility

		// remove the NULL charachter put there by C++
		switch parts[1][:len(parts[1])-1] {
		case "public":
			vis = Public
		case "private":
			vis = Private
		default:
			log.Println(parts, len(parts[1]))
			return table, fmt.Errorf("Annotation pass returned data in invalid format - entries")
		}

		table = append(table, SymbolTableEntry{Name: parts[0], ABI: SOS, Visibility: vis})
	}
	return table, nil
}

func processSymTab(file *elf.File, llir *os.File) (*SymbolTable, error) {
	symbols, err := file.Symbols()
	if err != nil {
		return nil, err
	}

	metadata, err := runLLVMPass("AnnotationPass", llir, nil, "-AnnotationPass")
	if err != nil {
		return nil, fmt.Errorf("Error %v %s", err, string(metadata))
	}

	stab, err := parseAnnotationPass(metadata)
	if err != nil {
		log.Println(string(metadata))
		return nil, err
	}

	for _, sym := range symbols {
		if elf.ST_BIND(sym.Info) == elf.STB_GLOBAL && elf.ST_TYPE(sym.Info) == elf.STT_FUNC {
			/* FIXME
			symentry, ok := stab[sym.Name]
			if ok {
				symentry.Offset = sym.Value
			}
			*/
		}
	}

	return &stab, nil
}

func produceManifest(binary *os.File, llir *os.File) error {
	ef, err := elf.NewFile(binary)
	if err != nil {
		return err
	}

	moduleName := replaceExtension(path.Base(binary.Name()), "")

	manifest := Manifest{ModuleName: moduleName}

	dotText := ef.Section(".text")
	if dotText == nil {
		return fmt.Errorf(".text section is missing")
	}
	/*
		dotRodata := ef.Section(".rodata")

			if dotRodata == nil {
				manifest.RoDataSize = 0
				manifest.RoDataAddr = 0
			} else {
				manifest.RoDataSize = dotRodata.Size
				manifest.RoDataAddr = dotRodata.Addr
			}
			manifest.TextSize = dotText.Size
	*/

	stab, err := processSymTab(ef, llir)
	if err != nil {
		return err
	}
	manifest.SymbolTable = *stab

	return injectManifest(binary, &manifest)
}

func runLLVMPass(pass string, input io.Reader, output *io.Writer, args ...string) ([]byte, error) {
	pass = PassPath + "/lib" + pass + ".so"

	outBuf := &bytes.Buffer{}
	cmdSplit := []string{"-load", pass}
	cmdSplit = append(cmdSplit, args...)

	cmd := exec.Command("opt", cmdSplit...)
	cmd.Stdin = input
	if output != nil {
		cmd.Stdout = *output
	} else {
		cmd.Stdout = outBuf
	}

	errBuf := &bytes.Buffer{}
	cmd.Stderr = errBuf

	err := cmd.Run()
	if err != nil {
		if output == nil {
			log.Println(string(outBuf.Bytes()))
		}
		return errBuf.Bytes(), err
	}

	return errBuf.Bytes(), nil
}

func checkBinary(binary *os.File) error {
	binary.Seek(0, os.SEEK_SET)
	ef, err := elf.NewFile(binary)
	if err != nil {
		return err
	}

	var dataSection *int
	for i, section := range ef.Sections {
		if strings.HasPrefix(section.Name, ".data") || strings.HasPrefix(section.Name, ".bss") {
			dataSection = &i
			break
		}
	}

	if dataSection != nil {
		dataSymbols := []string{}
		symbols, err := ef.Symbols()
		if err != nil {
			return err
		}

		for _, symbol := range symbols {
			if int(symbol.Section) == *dataSection {
				dataSymbols = append(dataSymbols, symbol.Name)
			}
		}

		return fmt.Errorf("Code contains data section with following symbols %v", dataSymbols)
	}

	return nil
}

func produceBinary(linked *os.File, arch string) (*os.File, error) {
	obj, err := ioutil.TempFile("", "compiling.o")
	if err != nil {
		return nil, err
	}
	defer os.Remove(obj.Name())

	cmd := exec.Command("llc", append(CompilerFlags[arch], "-filetype=obj", linked.Name(), "-o", obj.Name())...)

	errBuf := &bytes.Buffer{}
	cmd.Stderr = errBuf

	err = cmd.Run()
	if err != nil {
		return nil, fmt.Errorf("%v %s", err, string(errBuf.Bytes()))
	}

	binary, err := os.Create(replaceExtension(linked.Name(), "-"+arch+".o"))
	if err != nil {
		return nil, err
	}

	cmd = exec.Command("ld", "-T"+LinkerFile.Name(), obj.Name(), "-o", binary.Name())

	errBuf = &bytes.Buffer{}
	cmd.Stderr = errBuf

	err = cmd.Run()
	if err != nil {
		return binary, fmt.Errorf("%v %s", err, string(errBuf.Bytes()))
	}

	return binary, nil
}

func compileFile(file *os.File) error {
	log.Printf("Processing %s", file.Name())

	for _, arch := range Architechtures {
		binary, err := produceBinary(file, arch)
		if err != nil {
			return err
		}

		err = produceManifest(binary, file)
		if err != nil {
			return err
		}

		err = checkBinary(binary)
		if err != nil {
			return err
		}
	}

	return nil
}

func elfSymbols(binary *os.File) error {

	ef, err := elf.NewFile(binary)
	if err != nil {
		return err
	}

	symbols, err := ef.Symbols()
	if err != nil {
		return err
	}

	symtab := SymbolTable{}
	for _, sym := range symbols {
		/*
			if DEBUG {
				fmt.Printf("(%s-%s)%s@%s - %d\n", elf.ST_TYPE(sym.Info), elf.ST_BIND(sym.Info), sym.Name, sym.Section.String(), sym.Value)
			}
		*/
		if elf.ST_BIND(sym.Info) == elf.STB_GLOBAL && elf.ST_TYPE(sym.Info) == elf.STT_FUNC {
			symtab = append(symtab, SymbolTableEntry{Name: sym.Name, Offset: sym.Value, ABI: C, Visibility: Public})
		}
	}

	moduleName := replaceExtension(path.Base(binary.Name()), "")

	manifest := Manifest{ModuleName: moduleName, SymbolTable: symtab}

	return injectManifest(binary, &manifest)
}

func compileMain(c *cli.Context) error {
	inputFiles := c.Args()
	var err error

	LinkerFile, err = ioutil.TempFile("", "faastr.lds")
	if err != nil {
		return fmt.Errorf("Could not create temporary linker script")
	}
	defer os.Remove(LinkerFile.Name())

	if MergeData {
		_, err = LinkerFile.WriteString(LINKER_MERGE)
	} else {
		_, err = LinkerFile.WriteString(LINKER_NOMERGE)
	}

	if err != nil {
		return err
	}

	if InjectManifest {
		for _, file := range inputFiles {
			f, err := os.Open(file)
			if err != nil {
				return err
			}
			elfSymbols(f)
		}
		return nil
	}

	Stdlls, err = listStdLib()
	if err != nil {
		return err
	}

	for _, file := range inputFiles {
		f, err := os.Open(file)
		if err != nil {
			return err
		}
		if strings.HasSuffix(file, ".bc") || strings.HasSuffix(file, ".ll") {
			err = compileFile(f)
			if err != nil {
				return err
			}
		} else {
			log.Printf("Ignoring %s, not an llvm-ir file (!.bc && !.ll)", file)
		}
	}

	return nil
}

func main() {
	app := cli.NewApp()
	app.Name = "stage2"
	app.EnableBashCompletion = true
	app.Action = compileMain
	app.Flags = []cli.Flag{
		cli.BoolFlag{
			Name:        "just-manifest, m",
			Destination: &InjectManifest,
		},
		cli.BoolFlag{
			Name:        "output-linked-ll, l",
			Destination: &OutputLinkedLL,
		},
		cli.BoolFlag{
			Name:        "merge-data, d",
			Destination: &MergeData,
		},
		cli.StringFlag{
			Name:        "std-path, s",
			Value:       "stdlib",
			Destination: &StdPath,
		},
		cli.StringFlag{
			Name:        "pass-path, p",
			Value:       "stage2/passes",
			Destination: &PassPath,
		},
	}
	err := app.Run(os.Args)
	if err != nil {
		log.Fatal(err)
	}
}
