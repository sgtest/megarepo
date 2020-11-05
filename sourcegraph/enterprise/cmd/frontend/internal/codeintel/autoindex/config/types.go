package config

type IndexConfiguration struct {
	SharedSteps []DockerStep
	IndexJobs   []IndexJob
}

type IndexJob struct {
	Steps       []DockerStep
	Root        string
	Indexer     string
	IndexerArgs []string
	Outfile     string
}

type DockerStep struct {
	Root     string
	Image    string
	Commands []string
}
